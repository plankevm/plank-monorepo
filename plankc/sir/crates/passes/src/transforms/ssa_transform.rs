use crate::{AnalysesStore, Predecessors, run_pass, transforms::CriticalEdgeSplitting};
use hashbrown::{HashMap, HashSet};
use plank_core::{Idx, IncIterable};
use sir_data::{
    BasicBlock, BasicBlockId, Control, EthIRProgram, IndexVec, LocalId, Span, index_vec,
};
use smallvec::SmallVec;

type Defs = IndexVec<BasicBlockId, HashMap<LocalId, LocalId>>;

pub fn run(program: &mut EthIRProgram, store: &AnalysesStore) {
    // CriticalEdgeSplitting is stateless, no benefit from reuse
    run_pass(&mut CriticalEdgeSplitting, program, store);

    println!("{program}");

    let predecessors = store.predecessors(program);
    let reachability = store.reachability(program);

    let mut unfilled_until_sealed = HashMap::new();
    for bb in program.basic_blocks.iter_idx() {
        let pred_count = predecessors.of(bb).len() as u32;
        unsafe { unfilled_until_sealed.insert_unique_unchecked(bb, pred_count) };
    }

    let mut t = SSATransformer {
        outputs: index_vec![Vec::new(); program.basic_blocks.len()],
        inputs: index_vec![Vec::new(); program.basic_blocks.len()],
        defs: index_vec![HashMap::new(); program.basic_blocks.len()],
        predecessors: &predecessors,
        program,
        filled: HashSet::new(),
        unfilled_until_sealed,
    };

    let mut tmp_locals = SmallVec::<[LocalId; 32]>::new();

    // Use RPO so that only loop back edges require incomplete phis.
    for &bb in store.reverse_post_order(t.program).order() {
        println!("filling @{}", bb.get());

        for block_input in &mut t.program.locals[t.program.basic_blocks[bb].inputs] {
            let new_out = t.program.next_free_local_id.get_and_inc();
            let original = std::mem::replace(block_input, new_out);
            t.defs[bb].insert(original, new_out);
        }

        for op_idx in t.program.basic_blocks[bb].operations.iter() {
            let mut op = t.program.operations[op_idx];

            // Use temporary buffer to circumvent borrow checker.
            tmp_locals.clear();
            tmp_locals.extend_from_slice(op.inputs(t.program));
            for input in &mut tmp_locals {
                *input = t.read_variable(bb, *input);
            }
            op.inputs_mut(&mut t.program.locals).copy_from_slice(&tmp_locals);

            for output in op.outputs_mut(&mut t.program.locals, &t.program.functions) {
                let new_out = t.program.next_free_local_id.get_and_inc();
                let original = std::mem::replace(output, new_out);
                t.defs[bb].insert(original, new_out);
            }

            t.program.operations[op_idx] = op;
        }

        tmp_locals.clear();
        tmp_locals.extend_from_slice(&t.program.locals[t.program.basic_blocks[bb].outputs]);
        for block_output in &mut tmp_locals {
            *block_output = t.read_variable(bb, *block_output);
        }
        t.program.locals[t.program.basic_blocks[bb].outputs].copy_from_slice(&tmp_locals);

        match &mut t.program.basic_blocks[bb].control {
            Control::LastOpTerminates | Control::InternalReturn | Control::ContinuesTo(_) => {}
            Control::Branches(branches) => branches.condition = t.defs[bb][&branches.condition],
            Control::Switch(switch) => switch.condition = t.defs[bb][&switch.condition],
        }

        t.filled.insert(bb);
        for succ in t.program.block(bb).successors() {
            let unfilled = t.unfilled_until_sealed.get_mut(&succ).unwrap();
            *unfilled -= 1;
        }
        for oi in 0..t.outputs[bb].len() {
            match t.outputs[bb][oi] {
                PhiParam::Output(_) => {}
                PhiParam::Missing(missing) => {
                    t.outputs[bb][oi] = PhiParam::Output(t.read_variable(bb, missing));
                }
            }
        }
    }

    for bb in t.program.basic_blocks.iter_idx() {
        if !reachability.contains(bb) {
            continue;
        }
        let BasicBlock { inputs, outputs, .. } = t.program.basic_blocks[bb];
        let inputs_start = t.program.locals.len_idx();
        t.program.locals.extend_from_within(inputs.usize_range());
        t.program.locals.extend_from_slice(&t.inputs[bb]);
        t.program.basic_blocks[bb].inputs = Span::new(inputs_start, t.program.locals.len_idx());

        let outputs_start = t.program.locals.len_idx();
        t.program.locals.extend_from_within(outputs.usize_range());
        t.program.locals.extend(t.outputs[bb].iter().map(|out| match out {
            PhiParam::Output(out) => out,
            PhiParam::Missing(_) => unreachable!("unresolved missing param (@{})", bb.get()),
        }));
        t.program.basic_blocks[bb].outputs = Span::new(outputs_start, t.program.locals.len_idx());
    }
}

#[derive(Debug, Clone, Copy)]
enum PhiParam {
    Missing(LocalId),
    Output(LocalId),
}

struct SSATransformer<'a> {
    predecessors: &'a Predecessors,
    program: &'a mut EthIRProgram,
    unfilled_until_sealed: HashMap<BasicBlockId, u32>,
    filled: HashSet<BasicBlockId>,
    outputs: IndexVec<BasicBlockId, Vec<PhiParam>>,
    inputs: IndexVec<BasicBlockId, Vec<LocalId>>,
    defs: Defs,
}

impl SSATransformer<'_> {
    fn sealed(&self, bb: BasicBlockId) -> bool {
        match self.unfilled_until_sealed.get(&bb) {
            Some(&count) => count == 0,
            None => false,
        }
    }

    fn filled(&self, bb: BasicBlockId) -> bool {
        self.filled.contains(&bb)
    }

    fn read_variable(&mut self, bb: BasicBlockId, local: LocalId) -> LocalId {
        if let Some(&local) = self.defs[bb].get(&local) {
            return local;
        }

        if !self.sealed(bb) {
            let new_out = self.program.next_free_local_id.get_and_inc();
            self.defs[bb].insert(local, new_out);

            self.inputs[bb].push(new_out);
            for &pred in self.predecessors.of(bb) {
                let phi_param = if self.filled(pred) {
                    PhiParam::Output(self.read_variable(pred, local))
                } else {
                    PhiParam::Missing(local)
                };
                self.outputs[pred].push(phi_param);
            }
            return new_out;
        }

        if let &[single_pred] = self.predecessors.of(bb) {
            return self.read_variable(single_pred, local);
        }

        let new_out = self.program.next_free_local_id.get_and_inc();
        self.defs[bb].insert(local, new_out);

        self.inputs[bb].push(new_out);
        for &pred in self.predecessors.of(bb) {
            let path_input = self.read_variable(pred, local);
            self.outputs[pred].push(PhiParam::Output(path_input));
        }

        new_out
    }
}

#[cfg(test)]
mod tests {
    use super::{run as ssa_transform, *};
    use crate::Legalizer;
    use plank_test_utils::dedent_preserve_indent;
    use sir_parser::EmitConfig;

    fn assert_transforms_to(input: &str, expected_out: &str) {
        let mut config = EmitConfig::init_only();
        config.allow_duplicate_locals = true;
        let mut program = sir_parser::parse_without_legalization(input, config);
        let store = AnalysesStore::default();
        ssa_transform(&mut program, &store);
        Legalizer::default()
            .run(&mut program, &store)
            .unwrap_or_else(|e| panic!("Legalize failed:\n{e}\n{program}"));

        pretty_assertions::assert_str_eq!(
            dedent_preserve_indent(&format!("{program}")),
            dedent_preserve_indent(expected_out)
        );
    }

    #[test]
    fn test_unreachable() {
        assert_transforms_to(
            r#"
            fn init:
                @0 {
                    $0 = const 0x0
                    => $0 ? @1 : @4
                }

                @1 {
                    $0 = const 0x1
                    => @3
                }

                @2 {
                    $0 = const 0x2
                    => @3
                }

                @3 {
                    $2 = copy $0
                    => @4
                }

                @4 {
                    $1 = copy $0
                    stop
                }
            "#,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)
            Basic Blocks:
                @0 {
                    $3 = const 0x0
                    => $3 ? @1 : @5
                }
                @1 -> $4 {
                    $4 = const 0x1
                    => @3
                }
                @2 {
                    $0 = const 0x2
                    => @3
                }
                @3 $5 -> $5 {
                    $6 = copy $5
                    => @4
                }
                @4 $7 {
                    $8 = copy $7
                    stop
                }
                @5 -> $3 {
                    => @4
                }
            "#,
        );
    }

    #[test]
    fn test_branched_set() {
        assert_transforms_to(
            r#"
            fn init:
                @0 {
                    $0 = const 0x0
                    => $0 ? @1 : @5
                }

                @1 {
                    $21 = const 0x0
                    => $21 ? @2 : @3
                }

                @2 {
                    $0 = const 0x1
                    => @4
                }

                @3 {
                    $0 = const 0x2
                    => @4
                }

                @4 {
                    $2 = copy $0
                    => @5
                }

                @5 {
                    $1 = copy $0
                    stop
                }

            "#,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)
            Basic Blocks:
                @0 {
                    $4 = const 0x0
                    => $4 ? @1 : @6
                }
                @1 {
                    $5 = const 0x0
                    => $5 ? @2 : @3
                }
                @2 -> $6 {
                    $6 = const 0x1
                    => @4
                }
                @3 -> $7 {
                    $7 = const 0x2
                    => @4
                }
                @4 $8 -> $8 {
                    $9 = copy $8
                    => @5
                }
                @5 $10 {
                    $11 = copy $10
                    stop
                }
                @6 -> $4 {
                    => @5
                }
            "#,
        );
    }

    #[test]
    fn test_simple_merge() {
        assert_transforms_to(
            r#"
            fn init:
                @0 {
                    $0 = const 0x0
                    => $0 ? @1 : @5
                }

                @1 {
                    $1 = const 0x0
                    => $1 ? @2 : @3
                }

                @2 {
                    $2 = const 0x1
                    => @4
                }

                @3 {
                    $2 = const 0x2
                    => @4
                }

                @4 {
                    $3 = copy $2
                    => @5
                }

                @5 {
                    stop
                }
            "#,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)
            Basic Blocks:
                @0 {
                    $4 = const 0x0
                    => $4 ? @1 : @6
                }
                @1 {
                    $5 = const 0x0
                    => $5 ? @2 : @3
                }
                @2 -> $6 {
                    $6 = const 0x1
                    => @4
                }
                @3 -> $7 {
                    $7 = const 0x2
                    => @4
                }
                @4 $8 {
                    $9 = copy $8
                    => @5
                }
                @5 {
                    stop
                }
                @6 {
                    => @5
                }
            "#,
        );
    }

    #[test]
    fn test_loop_phi() {
        assert_transforms_to(
            r#"
            fn init:
                @0 {
                    $0 = const 0x0
                    => @1
                }

                @1 {
                    $1 = const 0x1
                    => $1 ? @2 : @3
                }

                @2 {
                    $2 = const 0x1
                    $0 = add $0 $2
                    => @1
                }

                @3 {
                    stop
                }
            "#,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)
            Basic Blocks:
                @0 -> $3 {
                    $3 = const 0x0
                    => @1
                }
                @1 $6 {
                    $4 = const 0x1
                    => $4 ? @2 : @3
                }
                @2 -> $7 {
                    $5 = const 0x1
                    $7 = add $6 $5
                    => @1
                }
                @3 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_multiple_phis_at_join() {
        assert_transforms_to(
            r#"
            fn init:
                @0 {
                    $0 = const 0x0
                    $1 = const 0x0
                    => $1 ? @1 : @2
                }

                @1 {
                    $0 = const 0x1
                    => @3
                }

                @2 {
                    $1 = const 0x4
                    => @3
                }

                @3 {
                    $3 = add $0 $1
                    stop
                }
            "#,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)
            Basic Blocks:
                @0 {
                    $3 = const 0x0
                    $4 = const 0x0
                    => $4 ? @1 : @2
                }
                @1 -> $5 $4 {
                    $5 = const 0x1
                    => @3
                }
                @2 -> $3 $6 {
                    $6 = const 0x4
                    => @3
                }
                @3 $7 $8 {
                    $9 = add $7 $8
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_icall_without_args() {
        assert_transforms_to(
            r#"
            fn init:
                @0 {
                    icall @other
                    stop
                }

            fn other:
                @0 {
                    cond = const 0
                    => cond ? @1 : @2
                }
                @1 {
                    a = const 1
                    => @3
                }
                @2 {
                    a = const 2
                    => @3
                }
                @3 {
                    sstore a a
                    iret
                }
            "#,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 0)
                fn @1 -> entry @4  (outputs: 0)
            Basic Blocks:
                @0 {
                    $2 = const 0x0
                    => $2 ? @1 : @2
                }
                @1 -> $3 {
                    $3 = const 0x1
                    => @3
                }
                @2 -> $4 {
                    $4 = const 0x2
                    => @3
                }
                @3 $5 {
                    sstore $5 $5
                    iret
                }
                @4 {
                    icall @0
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_icall_no_mut_args() {
        assert_transforms_to(
            r#"
            fn init:
                @0 {
                    in = const 0
                    out = icall @other in
                    stop
                }

            fn other:
                @0 in {
                    cond = const 0
                    => cond ? @1 : @2
                }
                @1 {
                    a = const 1
                    => @3
                }
                @2 {
                    a = const 2
                    => @3
                }
                @3 -> a {
                    iret
                }
            "#,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @4  (outputs: 0)
            Basic Blocks:
                @0 $7 {
                    $8 = const 0x0
                    => $8 ? @1 : @2
                }
                @1 -> $9 {
                    $9 = const 0x1
                    => @3
                }
                @2 -> $10 {
                    $10 = const 0x2
                    => @3
                }
                @3 $11 -> $11 {
                    iret
                }
                @4 {
                    $5 = const 0x0
                    $6 = icall @0 $5
                    stop
                }
            "#,
        );
    }
}
