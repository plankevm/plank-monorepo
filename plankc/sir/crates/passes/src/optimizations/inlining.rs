use hashbrown::HashMap;
use plank_core::{DenseIndexSet, IncIterable, IndexVec, Span};
use sir_data::{
    BasicBlock, BasicBlockId, Branch, Cases, Control, EthIRProgram, FunctionId, LocalId, LocalIdx,
    Operation, OperationIdx, Switch, operation::InternalCallData,
};

use crate::{AnalysesStore, Pass};

pub(crate) const DEFAULT_INLINE_SIZE_THRESHOLD: u32 = 8;

#[derive(Debug, Clone)]
pub(crate) struct Inliner {
    heuristic: DefaultHeuristic,
    callsites: IndexVec<FunctionId, Vec<OperationIdx>>,
    callsite_blocks: HashMap<OperationIdx, BasicBlockId>,
}

impl Pass for Inliner {
    fn run(&mut self, program: &mut EthIRProgram, store: &AnalysesStore) {
        let rpo = store.reverse_post_order(program);
        self.callsites.clear();
        self.callsites.resize_with(program.functions.len(), Vec::new);
        self.callsite_blocks.clear();

        for &block in rpo.blocks_rpo() {
            for operation_id in program.basic_blocks[block].operations.iter() {
                if let Operation::InternalCall(call) = program.operations[operation_id] {
                    self.callsites[call.function].push(operation_id);
                    self.callsite_blocks.insert(operation_id, block);
                }
            }
        }

        for &function_id in rpo.functions_postorder() {
            let should_inline = match self.callsites[function_id].len() {
                0 => false,
                1 => true,
                _ => self.heuristic.should_inline(program, function_id),
            };
            if should_inline {
                self.inline_function(program, function_id);
            }
        }
    }
}

impl Inliner {
    pub(crate) fn new(size_threshold: u32) -> Self {
        Self {
            heuristic: DefaultHeuristic::new(size_threshold),
            callsites: IndexVec::new(),
            callsite_blocks: HashMap::new(),
        }
    }

    fn inline_function(&mut self, program: &mut EthIRProgram, function_id: FunctionId) {
        while let Some(callsite_operation) = self.callsites[function_id].pop() {
            let callsite_block = self
                .callsite_blocks
                .remove(&callsite_operation)
                .expect("tracked callsite should have a current block");
            let Operation::InternalCall(call) = program.operations[callsite_operation] else {
                unreachable!("tracked callsite should point to an internal call")
            };
            assert_eq!(call.function, function_id);
            self.inline_callsite(program, callsite_block, callsite_operation, call);
        }
    }

    fn inline_callsite(
        &mut self,
        program: &mut EthIRProgram,
        callsite_block: BasicBlockId,
        callsite_operation: OperationIdx,
        call: InternalCallData,
    ) {
        let original_callsite_block = program.basic_blocks[callsite_block];

        program.basic_blocks[callsite_block].outputs = call.inputs_span();
        program.basic_blocks[callsite_block].operations =
            Span::new(original_callsite_block.operations.start, callsite_operation);

        let join_block = program.basic_blocks.push(BasicBlock {
            inputs: call.outputs_span(program),
            outputs: original_callsite_block.outputs,
            operations: Span::new(callsite_operation + 1, original_callsite_block.operations.end),
            control: original_callsite_block.control,
        });

        for moved_operation in program.basic_blocks[join_block].operations.iter() {
            if let Operation::InternalCall(moved_call) = program.operations[moved_operation] {
                let previous_callsite_block = self
                    .callsite_blocks
                    .insert(moved_operation, join_block)
                    .expect("internal call should have a tracked block");
                assert_eq!(previous_callsite_block, callsite_block);
                assert!(
                    self.callsites[moved_call.function].contains(&moved_operation),
                    "internal call should be a tracked callsite"
                );
            }
        }

        let callee_entry = program.functions[call.function].entry();
        let mut block_remapper = BlockRemapper::new(program, join_block);
        let remapped_entry = block_remapper.remap_block_id(callee_entry);
        while let Some(source_block) = block_remapper.block_worklist.pop() {
            block_remapper.remap_block(source_block);
        }
        program.basic_blocks[callsite_block].control = Control::ContinuesTo(remapped_entry);
    }
}

struct BlockRemapper<'a> {
    program: &'a mut EthIRProgram,
    return_target: BasicBlockId,
    remapped_blocks: HashMap<BasicBlockId, BasicBlockId>,
    remapped_locals: HashMap<LocalId, LocalId>,
    block_worklist: Vec<BasicBlockId>,
}

impl<'a> BlockRemapper<'a> {
    fn new(program: &'a mut EthIRProgram, return_target: BasicBlockId) -> Self {
        Self {
            program,
            return_target,
            remapped_blocks: HashMap::new(),
            remapped_locals: HashMap::new(),
            block_worklist: Vec::new(),
        }
    }

    fn remap_block_id(&mut self, source: BasicBlockId) -> BasicBlockId {
        if let Some(&destination) = self.remapped_blocks.get(&source) {
            return destination;
        }

        let destination = self.program.basic_blocks.push(BasicBlock {
            inputs: Span::EMPTY,
            outputs: Span::EMPTY,
            operations: Span::EMPTY,
            control: Control::LastOpTerminates,
        });
        self.remapped_blocks.insert(source, destination);
        self.block_worklist.push(source);
        destination
    }

    fn remap_block(&mut self, source_id: BasicBlockId) {
        let destination_id = self.remapped_blocks[&source_id];
        let source = self.program.basic_blocks[source_id];
        let remapped = BasicBlock {
            inputs: self.remap_locals(source.inputs),
            operations: self.remap_operations(source.operations),
            outputs: self.remap_locals(source.outputs),
            control: self.remap_control(source.control),
        };

        self.program.basic_blocks[destination_id] = remapped;
    }

    fn remap_locals(&mut self, source: Span<LocalIdx>) -> Span<LocalIdx> {
        let start = self.program.locals.next_idx();
        for source_idx in source.iter() {
            let source_local = self.program.locals[source_idx];
            let remapped_local = remap_local(
                &mut self.remapped_locals,
                &mut self.program.next_free_local_id,
                source_local,
            );
            self.program.locals.push(remapped_local);
        }
        Span::new(start, self.program.locals.next_idx())
    }

    fn remap_operations(&mut self, source: Span<OperationIdx>) -> Span<OperationIdx> {
        let start = self.program.operations.next_idx();
        for source_operation in source.iter() {
            let cloned_operation = self.program.clone_operation(source_operation);
            let mut operation = self.program.operations[cloned_operation];
            for input in operation.inputs_mut(&mut self.program.locals) {
                *input = remap_local(
                    &mut self.remapped_locals,
                    &mut self.program.next_free_local_id,
                    *input,
                );
            }
            for output in operation.outputs_mut(&mut self.program.locals, &self.program.functions) {
                *output = remap_local(
                    &mut self.remapped_locals,
                    &mut self.program.next_free_local_id,
                    *output,
                );
            }
            self.program.operations[cloned_operation] = operation;
        }
        Span::new(start, self.program.operations.next_idx())
    }

    fn remap_control(&mut self, source: Control) -> Control {
        match source {
            Control::LastOpTerminates => Control::LastOpTerminates,
            Control::InternalReturn => Control::ContinuesTo(self.return_target),
            Control::ContinuesTo(target) => Control::ContinuesTo(self.remap_block_id(target)),
            Control::Branches(branch) => {
                let zero_target = self.remap_block_id(branch.zero_target);
                let non_zero_target = self.remap_block_id(branch.non_zero_target);
                Control::Branches(Branch {
                    condition: self.remapped_locals[&branch.condition],
                    non_zero_target,
                    zero_target,
                })
            }
            Control::Switch(switch) => {
                let source_cases = self.program.cases[switch.cases];
                let targets_start_id = self.program.cases_bb_ids.next_idx();
                for source_target_idx in source_cases.target_indices().iter() {
                    let source_target = self.program.cases_bb_ids[source_target_idx];
                    let remapped_target = self.remap_block_id(source_target);
                    self.program.cases_bb_ids.push(remapped_target);
                }
                let cases = self.program.cases.push(Cases {
                    values_start_id: source_cases.values_start_id,
                    targets_start_id,
                    cases_count: source_cases.cases_count,
                });
                let fallback = switch.fallback.map(|target| self.remap_block_id(target));
                Control::Switch(Switch {
                    condition: self.remapped_locals[&switch.condition],
                    fallback,
                    cases,
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DefaultHeuristic {
    size_threshold: u32,
    visited_blocks: DenseIndexSet<BasicBlockId>,
    block_worklist: Vec<BasicBlockId>,
}

impl DefaultHeuristic {
    pub(crate) fn new(size_threshold: u32) -> Self {
        Self { size_threshold, visited_blocks: DenseIndexSet::new(), block_worklist: Vec::new() }
    }

    fn should_inline(&mut self, program: &EthIRProgram, function_id: FunctionId) -> bool {
        let mut size = 0;
        self.visited_blocks.clear();
        self.block_worklist.clear();
        self.block_worklist.push(program.functions[function_id].entry());

        while let Some(block) = self.block_worklist.pop() {
            if !self.visited_blocks.add(block) {
                continue;
            }

            size += program.basic_blocks[block].operations.len() + 1;
            if size > self.size_threshold {
                return false;
            }

            self.block_worklist.extend(program.basic_blocks[block].control.iter_outgoing(program));
        }

        true
    }
}

fn remap_local(
    remapped_locals: &mut HashMap<LocalId, LocalId>,
    next_free_local_id: &mut LocalId,
    local: LocalId,
) -> LocalId {
    *remapped_locals.entry(local).or_insert_with(|| next_free_local_id.get_and_inc())
}

#[cfg(test)]
mod tests {
    use super::Inliner;
    use crate::{AnalysesStore, Defragmenter, Legalizer, run_pass};
    use sir_data::{Operation, assert_ir_display};
    use sir_parser::{EmitConfig, parse_or_panic};

    fn inline(source: &str) -> sir_data::EthIRProgram {
        let mut program = parse_or_panic(source, EmitConfig::init_only());
        let store = AnalysesStore::default();
        run_pass(&mut Inliner::new(4), &mut program, &store);
        Legalizer::default()
            .run(&program, &store)
            .unwrap_or_else(|err| panic!("legalization failed after inlining: {err}\n{program}"));
        program
    }

    #[test]
    fn test_single_callsite() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    x = const 2
                    y = icall @double x
                    stop
                }

            fn double:
                entry x -> y {
                    y = add x x
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $1 {
                    $1 = add $0 $0
                    iret
                }

                @1 -> $2 {
                    $2 = const 0x2
                    => @3
                }

                @2 $3 {
                    stop
                }

                @3 $4 -> $5 {
                    $5 = add $4 $4
                    => @2
                }
            "#,
        );
    }

    #[test]
    fn test_nested_inlining_and_defragmentation() {
        let mut actual = inline(
            r#"
            fn init:
                entry {
                    x = const 5
                    y = icall @sum_inc x
                    stop
                }

            fn sum_inc:
                entry x -> y {
                    one = const 1
                    inc = icall @add x one
                    y = add inc x
                    iret
                }

            fn add:
                entry a b -> sum {
                    sum = add a b
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @2
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 1)
                fn @2 -> entry @2  (outputs: 0)

            Basic Blocks:
                @0 $0 $1 -> $2 {
                    $2 = add $0 $1
                    iret
                }

                @1 $3 -> $3 $4 {
                    $4 = const 0x1
                    => @4
                }

                @2 -> $7 {
                    $7 = const 0x5
                    => @6
                }

                @3 $5 -> $6 {
                    $6 = add $5 $3
                    iret
                }

                @4 $9 $10 -> $11 {
                    $11 = add $9 $10
                    => @3
                }

                @5 $8 {
                    stop
                }

                @6 $12 -> $12 $13 {
                    $13 = const 0x1
                    => @7
                }

                @7 $14 $15 -> $16 {
                    $16 = add $14 $15
                    => @8
                }

                @8 $17 -> $18 {
                    $18 = add $17 $12
                    => @5
                }
            "#,
        );

        let store = AnalysesStore::default();
        run_pass(&mut Defragmenter::default(), &mut actual, &store);
        Legalizer::default().run(&actual, &store).unwrap_or_else(|err| {
            panic!("legalization failed after defragmentation: {err}\n{actual}")
        });

        assert_ir_display(
            &actual,
            r#"
            Init: @0
            Functions:
                fn @0 -> entry @0  (outputs: 0)

            Basic Blocks:
                @0 -> $0 {
                    $0 = const 0x5
                    => @1
                }

                @1 $1 -> $1 $2 {
                    $2 = const 0x1
                    => @2
                }

                @2 $3 $4 -> $5 {
                    $5 = add $3 $4
                    => @3
                }

                @3 $6 -> $7 {
                    $7 = add $6 $1
                    => @4
                }

                @4 $8 {
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_inlining_remaps_allocated_operation_inputs() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    a = const 2
                    b = const 3
                    modulus = const 5
                    result = icall @add_mod a b modulus
                    stop
                }

            fn add_mod:
                entry a b modulus -> result {
                    result = addmod a b modulus
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 0)

            Basic Blocks:
                @0 $0 $1 $2 -> $3 {
                    $3 = addmod $0 $1 $2
                    iret
                }

                @1 -> $4 $5 $6 {
                    $4 = const 0x2
                    $5 = const 0x3
                    $6 = const 0x5
                    => @3
                }

                @2 $7 {
                    stop
                }

                @3 $8 $9 $10 -> $11 {
                    $11 = addmod $8 $9 $10
                    => @2
                }
            "#,
        );
    }

    #[test]
    fn test_inlining_remaps_memory_operations() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    value = const 7
                    loaded = icall @round_trip value
                    stop
                }

            fn round_trip:
                entry value -> value {
                    => @access_memory
                }
                access_memory memory_value -> loaded {
                    size = const 32
                    ptr = malloc size
                    mstore256 ptr memory_value
                    loaded = mload256 ptr
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @2  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $0 {
                    => @1
                }

                @1 $1 -> $4 {
                    $2 = const 0x20
                    $3 = malloc $2
                    mstore256 $3 $1
                    $4 = mload256 $3
                    iret
                }

                @2 -> $5 {
                    $5 = const 0x7
                    => @4
                }

                @3 $6 {
                    stop
                }

                @4 $7 -> $7 {
                    => @5
                }

                @5 $8 -> $11 {
                    $9 = const 0x20
                    $10 = malloc $9
                    mstore256 $10 $8
                    $11 = mload256 $10
                    => @3
                }
            "#,
        );
    }

    #[test]
    fn test_single_callsite_ignores_size_threshold() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    x = const 1
                    result = icall @large x
                    stop
                }

            fn large:
                entry x -> result {
                    a = add x x
                    b = add a x
                    c = add b x
                    result = add c x
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $4 {
                    $1 = add $0 $0
                    $2 = add $1 $0
                    $3 = add $2 $0
                    $4 = add $3 $0
                    iret
                }

                @1 -> $5 {
                    $5 = const 0x1
                    => @3
                }

                @2 $6 {
                    stop
                }

                @3 $7 -> $11 {
                    $8 = add $7 $7
                    $9 = add $8 $7
                    $10 = add $9 $7
                    $11 = add $10 $7
                    => @2
                }
            "#,
        );
    }

    #[test]
    fn test_multiple_callsites_over_size_threshold_are_not_inlined() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    x = const 1
                    a = icall @large x
                    b = icall @large a
                    stop
                }

            fn large:
                entry x -> result {
                    a = add x x
                    b = add a x
                    c = add b x
                    result = add c x
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $4 {
                    $1 = add $0 $0
                    $2 = add $1 $0
                    $3 = add $2 $0
                    $4 = add $3 $0
                    iret
                }

                @1 {
                    $5 = const 0x1
                    $6 = icall @0 $5
                    $7 = icall @0 $6
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_inlining_remaps_retained_internal_call() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    x = const 1
                    wrapped = icall @wrapper x
                    direct = icall @large wrapped
                    stop
                }

            fn wrapper:
                entry x -> x {
                    => @call_large
                }
                call_large call_x -> result {
                    result = icall @large call_x
                    iret
                }

            fn large:
                entry x -> result {
                    a = add x x
                    b = add a x
                    c = add b x
                    result = add c x
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @2
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 1)
                fn @2 -> entry @3  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $4 {
                    $1 = add $0 $0
                    $2 = add $1 $0
                    $3 = add $2 $0
                    $4 = add $3 $0
                    iret
                }

                @1 $5 -> $5 {
                    => @2
                }

                @2 $6 -> $7 {
                    $7 = icall @0 $6
                    iret
                }

                @3 -> $8 {
                    $8 = const 0x1
                    => @5
                }

                @4 $9 {
                    $10 = icall @0 $9
                    stop
                }

                @5 $11 -> $11 {
                    => @6
                }

                @6 $12 -> $13 {
                    $13 = icall @0 $12
                    => @4
                }
            "#,
        );
    }

    #[test]
    fn test_multiple_callsites_at_size_threshold_are_inlined() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    x = const 1
                    a = icall @small x
                    b = icall @small a
                    stop
                }

            fn small:
                entry x -> result {
                    a = add x x
                    b = add a x
                    result = add b x
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $3 {
                    $1 = add $0 $0
                    $2 = add $1 $0
                    $3 = add $2 $0
                    iret
                }

                @1 -> $4 {
                    $4 = const 0x1
                    => @5
                }

                @2 $6 {
                    stop
                }

                @3 $7 -> $10 {
                    $8 = add $7 $7
                    $9 = add $8 $7
                    $10 = add $9 $7
                    => @2
                }

                @4 $5 -> $5 {
                    => @3
                }

                @5 $11 -> $14 {
                    $12 = add $11 $11
                    $13 = add $12 $11
                    $14 = add $13 $11
                    => @4
                }
            "#,
        );
    }

    #[test]
    fn test_shared_callee_is_inlined_in_init_and_main() {
        let mut actual = parse_or_panic(
            r#"
            fn init:
                entry {
                    x = const 1
                    y = icall @shared x
                    stop
                }

            fn main:
                entry {
                    x = const 2
                    y = icall @shared x
                    stop
                }

            fn shared:
                entry x -> result {
                    result = add x x
                    iret
                }
            "#,
            EmitConfig::default(),
        );
        let store = AnalysesStore::default();
        run_pass(&mut Inliner::new(4), &mut actual, &store);
        Legalizer::default()
            .run(&actual, &store)
            .unwrap_or_else(|err| panic!("legalization failed after inlining: {err}\n{actual}"));

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Run: @2
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 0)
                fn @2 -> entry @2  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $1 {
                    $1 = add $0 $0
                    iret
                }

                @1 -> $2 {
                    $2 = const 0x1
                    => @4
                }

                @2 -> $4 {
                    $4 = const 0x2
                    => @6
                }

                @3 $3 {
                    stop
                }

                @4 $6 -> $7 {
                    $7 = add $6 $6
                    => @3
                }

                @5 $5 {
                    stop
                }

                @6 $8 -> $9 {
                    $9 = add $8 $8
                    => @5
                }
            "#,
        );
    }

    #[test]
    fn test_repeated_callsites_in_same_block() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    x = const 3
                    y = icall @increment x
                    z = icall @increment y
                    stop
                }

            fn increment:
                entry x -> x {
                    => @add_one
                }
                add_one add_one_x -> result {
                    one = const 1
                    result = add add_one_x one
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @2  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $0 {
                    => @1
                }

                @1 $1 -> $3 {
                    $2 = const 0x1
                    $3 = add $1 $2
                    iret
                }

                @2 -> $4 {
                    $4 = const 0x3
                    => @7
                }

                @3 $6 {
                    stop
                }

                @4 $7 -> $7 {
                    => @5
                }

                @5 $8 -> $10 {
                    $9 = const 0x1
                    $10 = add $8 $9
                    => @3
                }

                @6 $5 -> $5 {
                    => @4
                }

                @7 $11 -> $11 {
                    => @8
                }

                @8 $12 -> $14 {
                    $13 = const 0x1
                    $14 = add $12 $13
                    => @6
                }
            "#,
        );
    }

    #[test]
    fn test_inlining_updates_moved_callsite_tracking() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    cond = const 1
                    x = const 7
                    y = icall @choose cond x
                    z = icall @id y
                    => cond ? @yes : @no
                }
                yes { stop }
                no { stop }

            fn choose:
                entry cond x -> x {
                    => cond ? @ret : @zero
                }
                ret y -> y {
                    iret
                }
                zero _ -> z {
                    z = const 0
                    iret
                }

            fn id:
                entry x -> x {
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @2
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @3  (outputs: 1)
                fn @2 -> entry @4  (outputs: 0)

            Basic Blocks:
                @0 $0 $1 -> $1 {
                    => $0 ? @1 : @2
                }

                @1 $2 -> $2 {
                    iret
                }

                @2 $3 -> $4 {
                    $4 = const 0x0
                    iret
                }

                @3 $5 -> $5 {
                    iret
                }

                @4 -> $6 $7 {
                    $6 = const 0x1
                    $7 = const 0x7
                    => @8
                }

                @5 {
                    stop
                }

                @6 {
                    stop
                }

                @7 $8 -> $8 {
                    => @12
                }

                @8 $10 $11 -> $11 {
                    => $10 ? @10 : @9
                }

                @9 $13 -> $14 {
                    $14 = const 0x0
                    => @7
                }

                @10 $12 -> $12 {
                    => @7
                }

                @11 $9 {
                    => $6 ? @5 : @6
                }

                @12 $15 -> $15 {
                    => @11
                }
            "#,
        );
    }

    #[test]
    fn test_inlining_handles_switch_loop() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    selector = const 1
                    x = const 3
                    returned_selector y = icall @countdown selector x
                    difference = sub y returned_selector
                    stop
                }

            fn countdown:
                entry selector x -> selector x {
                    => @loop
                }
                loop loop_selector loop_x -> loop_selector next {
                    one = const 1
                    next = sub loop_x one
                    switch loop_x {
                        0 => @done
                        default => @loop
                    }
                }
                done done_selector done_x -> done_selector done_x {
                    iret
                }
            "#,
        );

        assert_ir_display(
            &actual,
            r#"
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 2)
                fn @1 -> entry @3  (outputs: 0)

            Basic Blocks:
                @0 $0 $1 -> $0 $1 {
                    => @1
                }

                @1 $2 $3 -> $2 $5 {
                    $4 = const 0x1
                    $5 = sub $3 $4
                    switch $3 {
                        0x0 => @2,
                        else => @1
                    }

                }

                @2 $6 $7 -> $6 $7 {
                    iret
                }

                @3 -> $8 $9 {
                    $8 = const 0x1
                    $9 = const 0x3
                    => @5
                }

                @4 $10 $11 {
                    $12 = sub $11 $10
                    stop
                }

                @5 $13 $14 -> $13 $14 {
                    => @6
                }

                @6 $15 $16 -> $15 $18 {
                    $17 = const 0x1
                    $18 = sub $16 $17
                    switch $16 {
                        0x0 => @7,
                        else => @6
                    }

                }

                @7 $19 $20 -> $19 $20 {
                    => @4
                }
            "#,
        );
    }

    #[test]
    fn test_inlining_preserves_static_allocation_identity() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    a = icall @alloc
                    b = icall @alloc
                    same = eq a b
                    stop
                }

            fn alloc:
                entry -> ptr {
                    ptr = salloc 32
                    => @done
                }
                done returned_ptr -> returned_ptr {
                    iret
                }
            "#,
        );

        let alloc_ids = actual
            .basic_blocks
            .iter()
            .flat_map(|block| block.operations.iter())
            .filter_map(|operation| match actual.operations[operation] {
                Operation::StaticAllocZeroed(data) => Some(data.alloc_id),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(alloc_ids.len(), 3);
        assert!(alloc_ids.iter().all(|&alloc_id| alloc_id == alloc_ids[0]));
    }
}
