use crate::{
    AnalysesStore, Pass, analyses::DominanceFrontiers, run_pass, transforms::CriticalEdgeSplitting,
};
use hashbrown::HashSet;
use plank_core::IncIterable;
use sir_data::{BasicBlockId, Control, EthIRProgram, Idx, IndexVec, LocalId, Span, index_vec};

#[derive(Default)]
pub struct SsaTransform {
    def_sites: IndexVec<LocalId, HashSet<BasicBlockId>>,
    dominators: IndexVec<BasicBlockId, Vec<BasicBlockId>>,
    phi_locations: IndexVec<BasicBlockId, Vec<LocalId>>,
    worklist: Vec<BasicBlockId>,
}

impl Pass for SsaTransform {
    fn run(&mut self, program: &mut EthIRProgram, store: &AnalysesStore) {
        // CriticalEdgeSplitting is stateless, no benefit from reuse
        run_pass(&mut CriticalEdgeSplitting, program, store);

        debug_assert!(self.worklist.is_empty());
        for parent in self.dominators.iter_mut() {
            parent.clear();
        }
        self.dominators.resize(program.basic_blocks.len(), Vec::new());

        let dominators_child_to_parent = store.dominators(program);
        for bb in program.basic_blocks.iter_idx() {
            if let Some(parent) = dominators_child_to_parent.of(bb)
                && parent != bb
            {
                self.dominators[parent].push(bb);
            }
        }

        self.collect_definition_sites(program);

        self.phi_locations.clear();
        self.phi_locations.resize(program.basic_blocks.len(), Vec::new());
        self.compute_phi_locations(&store.dominance_frontiers(program));

        self.rename(program);
    }
}

impl SsaTransform {
    fn collect_definition_sites(&mut self, program: &EthIRProgram) {
        self.def_sites.clear();
        self.def_sites.resize(program.next_free_local_id.idx(), HashSet::new());
        for block in program.blocks() {
            for &local in block.inputs() {
                self.def_sites[local].insert(block.id());
            }
            for op in block.operations() {
                for &local in op.outputs() {
                    self.def_sites[local].insert(block.id());
                }
            }
        }
    }

    fn compute_phi_locations(&mut self, dominance_frontiers: &DominanceFrontiers) {
        for (local, def_blocks) in self.def_sites.enumerate_idx() {
            if def_blocks.len() <= 1 {
                continue;
            }
            for bb in def_blocks {
                self.worklist.push(*bb);
            }
            while let Some(bb) = self.worklist.pop() {
                for &frontier_block in dominance_frontiers.of(bb) {
                    if !self.phi_locations[frontier_block].contains(&local) {
                        self.phi_locations[frontier_block].push(local);
                        self.worklist.push(frontier_block);
                    }
                }
            }
        }
    }

    fn rename(&self, program: &mut EthIRProgram) {
        let num_locals = program.next_free_local_id.idx();
        let mut local_versions = index_vec![Vec::new(); num_locals];
        let mut rename_trail = Vec::new();
        for func_id in program.functions.iter_idx() {
            self.rename_block(
                program,
                program.functions[func_id].entry(),
                &mut local_versions,
                &mut rename_trail,
            );
        }
    }

    fn rename_block(
        &self,
        program: &mut EthIRProgram,
        bb: BasicBlockId,
        local_versions: &mut IndexVec<LocalId, Vec<LocalId>>,
        rename_trail: &mut Vec<LocalId>,
    ) {
        let checkpoint = rename_trail.len();

        self.rename_block_inputs(program, bb, local_versions, rename_trail);

        rename_operations(program, bb, local_versions, rename_trail);

        match &mut program.basic_blocks[bb].control {
            Control::Branches(branch) => {
                branch.condition = rename_use(local_versions, branch.condition);
            }
            Control::Switch(switch) => {
                switch.condition = rename_use(local_versions, switch.condition);
            }
            _ => {}
        }

        self.rename_block_outputs(program, bb, local_versions);

        for child in &self.dominators[bb] {
            self.rename_block(program, *child, local_versions, rename_trail);
        }

        for local in &rename_trail[checkpoint..] {
            local_versions[*local].pop();
        }
        rename_trail.truncate(checkpoint);
    }

    fn rename_block_inputs(
        &self,
        program: &mut EthIRProgram,
        bb: BasicBlockId,
        local_versions: &mut IndexVec<LocalId, Vec<LocalId>>,
        rename_trail: &mut Vec<LocalId>,
    ) {
        let old_inputs_span = program.basic_blocks[bb].inputs;
        let new_inputs_start = program.locals.next_idx();
        for idx in old_inputs_span.iter() {
            let local = program.locals[idx];
            let renamed =
                rename_def(local_versions, rename_trail, &mut program.next_free_local_id, local);
            program.locals.push(renamed);
        }
        for local in &self.phi_locations[bb] {
            let renamed =
                rename_def(local_versions, rename_trail, &mut program.next_free_local_id, *local);
            program.locals.push(renamed);
        }
        program.basic_blocks[bb].inputs = Span::new(new_inputs_start, program.locals.next_idx());
    }

    fn rename_block_outputs(
        &self,
        program: &mut EthIRProgram,
        bb: BasicBlockId,
        local_versions: &IndexVec<LocalId, Vec<LocalId>>,
    ) {
        let old_outputs_span = program.basic_blocks[bb].outputs;
        let new_outputs_start = program.locals.next_idx();
        for idx in old_outputs_span.iter() {
            program.locals.push(rename_use(local_versions, program.locals[idx]));
        }
        // After critical edge splitting, only single-successor blocks (ContinuesTo) can
        // target a join block with phis. This lets us avoid collecting successors into a Vec.
        if let Control::ContinuesTo(succ) = program.basic_blocks[bb].control {
            for local in &self.phi_locations[succ] {
                program.locals.push(rename_use(local_versions, *local));
            }
        }
        program.basic_blocks[bb].outputs = Span::new(new_outputs_start, program.locals.next_idx());
    }
}

fn rename_operations(
    program: &mut EthIRProgram,
    bb: BasicBlockId,
    local_versions: &mut IndexVec<LocalId, Vec<LocalId>>,
    rename_trail: &mut Vec<LocalId>,
) {
    for op_idx in program.basic_blocks[bb].operations.iter() {
        let mut op = program.operations[op_idx];

        for input in op.inputs_mut(&mut program.locals) {
            *input = rename_use(local_versions, *input);
        }

        for output in op.outputs_mut(&mut program.locals, &program.functions) {
            *output =
                rename_def(local_versions, rename_trail, &mut program.next_free_local_id, *output);
        }

        program.operations[op_idx] = op;
    }
}

fn rename_use(local_versions: &IndexVec<LocalId, Vec<LocalId>>, local: LocalId) -> LocalId {
    *local_versions[local].last().expect("local not in scope")
}

fn rename_def(
    local_versions: &mut IndexVec<LocalId, Vec<LocalId>>,
    rename_trail: &mut Vec<LocalId>,
    next_local: &mut LocalId,
    local: LocalId,
) -> LocalId {
    let new_version = next_local.get_and_inc();
    local_versions[local].push(new_version);
    rename_trail.push(local);
    new_version
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Legalizer;
    use sir_data::display_program;
    use sir_parser::EmitConfig;

    fn parse_without_ssa(source: &str) -> EthIRProgram {
        let mut config = EmitConfig::init_only();
        config.allow_duplicate_locals = true;
        sir_parser::parse_without_legalization(source, config)
    }

    fn transform_and_legalize(program: &mut EthIRProgram, store: &AnalysesStore) {
        run_pass(&mut SsaTransform::default(), program, store);
        let ir = display_program(program);
        Legalizer::default().run(program, store).unwrap_or_else(|e| panic!("{e}\n{ir}"));
    }

    #[test]
    fn test_diamond_phi_placement() {
        //       A
        //      / \
        //     B   C
        //      \ /
        //       D
        //
        // v defined in B and C.
        let mut program = parse_without_ssa(
            r#"
            fn init:
                a {
                    cond = const 1
                    => cond ? @b : @c
                }
                b -> v {
                    v = const 2
                    => @d
                }
                c -> v {
                    v = const 3
                    => @d
                }
                d v {
                    stop
                }
            "#,
        );

        let d = BasicBlockId::new(3);
        let original_v = program.block(d).inputs()[0];

        transform_and_legalize(&mut program, &AnalysesStore::default());
        let post_ir = display_program(&program);

        let d_inputs = program.block(d).inputs();
        assert_eq!(d_inputs.len(), 2, "D should have original input + phi\n{post_ir}");
        for &input in d_inputs {
            assert_ne!(input, original_v, "phi input should be renamed\n{post_ir}");
        }
        assert_ne!(d_inputs[0], d_inputs[1], "phi inputs should be distinct\n{post_ir}");
    }

    #[test]
    fn test_partial_redef_phi() {
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        //
        // v defined in A and B, but not C.
        let mut program = parse_without_ssa(
            r#"
            fn init:
                a -> v {
                    v = const 1
                    cond = const 0
                    => cond ? @b : @c
                }
                b v -> v {
                    v = const 2
                    => @d
                }
                c v -> v {
                    => @d
                }
                d v {
                    stop
                }
            "#,
        );

        let d = BasicBlockId::new(3);
        let original_v = program.block(d).inputs()[0];

        transform_and_legalize(&mut program, &AnalysesStore::default());
        let post_ir = display_program(&program);

        let d_inputs = program.block(d).inputs();
        assert_eq!(d_inputs.len(), 2, "D should have original input + phi\n{post_ir}");
        for &input in d_inputs {
            assert_ne!(input, original_v, "phi input should be renamed\n{post_ir}");
        }
        assert_ne!(d_inputs[0], d_inputs[1], "phi inputs should be distinct\n{post_ir}");
    }

    #[test]
    fn test_loop_phi() {
        //   A
        //   |
        //   B <--+
        //  / \   |
        // D   C--+
        //
        // v defined in A and C.
        let mut program = parse_without_ssa(
            r#"
            fn init:
                a -> v {
                    v = const 0
                    => @b
                }
                b v -> v {
                    cond = const 1
                    => cond ? @c : @d
                }
                c v -> v {
                    one = const 1
                    v = add v one
                    => @b
                }
                d v {
                    stop
                }
            "#,
        );

        let b = BasicBlockId::new(1);
        let original_v = program.block(b).inputs()[0];

        transform_and_legalize(&mut program, &AnalysesStore::default());
        let post_ir = display_program(&program);

        let b_inputs = program.block(b).inputs();
        assert_eq!(b_inputs.len(), 2, "B should have original input + phi\n{post_ir}");
        for &input in b_inputs {
            assert_ne!(input, original_v, "phi input should be renamed\n{post_ir}");
        }
        assert_ne!(b_inputs[0], b_inputs[1], "phi inputs should be distinct\n{post_ir}");
    }

    #[test]
    fn test_multiple_phis_at_join() {
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        //
        // x defined in A and B, y defined in A and C.
        let mut program = parse_without_ssa(
            r#"
            fn init:
                a -> x y {
                    x = const 1
                    y = const 2
                    cond = const 0
                    => cond ? @b : @c
                }
                b x y -> x y {
                    x = const 3
                    => @d
                }
                c x y -> x y {
                    y = const 4
                    => @d
                }
                d x y {
                    stop
                }
            "#,
        );

        let d = BasicBlockId::new(3);
        let original_x = program.block(d).inputs()[0];
        let original_y = program.block(d).inputs()[1];

        transform_and_legalize(&mut program, &AnalysesStore::default());
        let post_ir = display_program(&program);

        let d_inputs = program.block(d).inputs();
        assert_eq!(d_inputs.len(), 4, "D should have 2 original inputs + 2 phis\n{post_ir}");
        for &input in d_inputs {
            assert_ne!(input, original_x, "x should be renamed\n{post_ir}");
            assert_ne!(input, original_y, "y should be renamed\n{post_ir}");
        }
    }

    #[test]
    fn test_icall_and_multi_function() {
        //  init:        helper:
        //     A           E
        //    / \          |
        //   B   C         F
        //    \ /
        //     D (calls helper with v)
        //
        // v defined in A and B. D calls helper passing v.
        let mut program = parse_without_ssa(
            r#"
            fn init:
                a -> v {
                    v = const 1
                    cond = const 0
                    => cond ? @b : @c
                }
                b v -> v {
                    v = const 2
                    => @d
                }
                c v -> v {
                    => @d
                }
                d v {
                    result = icall @helper v
                    stop
                }
            fn helper:
                e x -> x {
                    => @f
                }
                f x -> x {
                    iret
                }
            "#,
        );

        let init_entry = program.functions[program.init_entry].entry();
        let helper_id = program.functions.iter_idx().find(|&id| id != program.init_entry).unwrap();
        let helper_entry = program.functions[helper_id].entry();

        transform_and_legalize(&mut program, &AnalysesStore::default());
        let post_ir = display_program(&program);

        let init_inputs = program.block(init_entry).inputs();
        let helper_inputs = program.block(helper_entry).inputs();
        assert_eq!(helper_inputs.len(), 1, "helper entry should still have 1 input\n{post_ir}");
        for &init_local in init_inputs {
            for &helper_local in helper_inputs {
                assert_ne!(
                    init_local, helper_local,
                    "locals across functions should be renamed independently\n{post_ir}"
                );
            }
        }
    }
}
