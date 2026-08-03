use hashbrown::HashMap;
use plank_core::{DenseIndexSet, IncIterable, IndexVec, Span, index_vec};
use sir_data::{
    BasicBlock, BasicBlockId, Branch, Cases, Control, EthIRProgram, FunctionId, LocalId, Operation,
    OperationIdx, Switch,
    operation::{InlineOperands, InternalCallData, OpVisitorMut},
};

use crate::{AnalysesStore, Pass};

pub(crate) const DEFAULT_INLINE_SIZE_THRESHOLD: u32 = 8;

#[derive(Debug, Clone)]
pub(crate) struct Inliner<H> {
    heuristic: H,
    postorder: Vec<FunctionId>,
    callsites: IndexVec<FunctionId, Vec<OperationIdx>>,
    callsite_blocks: HashMap<OperationIdx, BasicBlockId>,
    remapped_locals: HashMap<LocalId, LocalId>,
}

impl<H: InlineHeuristic> Pass for Inliner<H> {
    fn run(&mut self, program: &mut EthIRProgram, _store: &AnalysesStore) {
        self.build_call_graph(program);
        for function_id in std::mem::take(&mut self.postorder) {
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

impl<H: InlineHeuristic> Inliner<H> {
    pub(crate) fn new(heuristic: H) -> Self {
        Self {
            heuristic,
            postorder: Vec::new(),
            callsites: IndexVec::new(),
            callsite_blocks: HashMap::new(),
            remapped_locals: HashMap::new(),
        }
    }

    fn build_call_graph(&mut self, program: &EthIRProgram) {
        self.postorder.clear();
        self.callsites.clear();
        self.callsites.resize_with(program.functions.len(), Vec::new);
        self.callsite_blocks.clear();

        let mut function_states = index_vec![FunctionState::NotStarted; program.functions.len()];

        walk_call_graph(
            program,
            program.init_entry,
            &mut function_states,
            &mut self.postorder,
            &mut self.callsites,
            &mut self.callsite_blocks,
        );
        if let Some(main_entry) = program.main_entry {
            walk_call_graph(
                program,
                main_entry,
                &mut function_states,
                &mut self.postorder,
                &mut self.callsites,
                &mut self.callsite_blocks,
            );
        }
    }

    fn inline_function(&mut self, program: &mut EthIRProgram, function_id: FunctionId) {
        let entry = program.functions[function_id].entry();
        let is_linear = matches!(program.basic_blocks[entry].control, Control::InternalReturn);
        while let Some(callsite_operation) = self.callsites[function_id].pop() {
            let callsite_block = self
                .callsite_blocks
                .remove(&callsite_operation)
                .expect("tracked callsite should have a current block");
            let Operation::InternalCall(call) = program.operations[callsite_operation] else {
                unreachable!("tracked callsite should point to an internal call")
            };
            assert_eq!(call.function, function_id);
            if is_linear {
                self.inline_linear_callsite(program, callsite_block, callsite_operation, call);
            } else {
                self.inline_cfg_callsite(program, callsite_block, callsite_operation, call);
            }
        }
    }

    fn inline_linear_callsite(
        &mut self,
        program: &mut EthIRProgram,
        callsite_block: BasicBlockId,
        callsite_operation: OperationIdx,
        call: InternalCallData,
    ) {
        let old_operations = program.basic_blocks[callsite_block].operations;
        let new_operations_start = program.operations.next_idx();
        self.remapped_locals.clear();

        for old_operation in old_operations.iter() {
            if old_operation == callsite_operation {
                let callee_entry = program.functions[call.function].entry();
                let callee_operations = program.basic_blocks[callee_entry].operations;
                let callee_inputs = program.block(callee_entry).inputs();
                let call_inputs = call.get_inputs(program);
                for input_index in 0..callee_inputs.len() {
                    self.remapped_locals
                        .insert(callee_inputs[input_index], call_inputs[input_index]);
                }

                for callee_operation in callee_operations.iter() {
                    let mut remapped_operation = program.operations[callee_operation];
                    remapped_operation.visit_data_mut(&mut OperationRemapper {
                        program,
                        remapped_locals: &mut self.remapped_locals,
                    });
                    program.operations.push(remapped_operation);
                }

                for output_index in 0..program.block(callee_entry).outputs().len() {
                    let callee_output = program.block(callee_entry).outputs()[output_index];
                    let call_output = call.get_outputs(program)[output_index];
                    program.operations.push(Operation::SetCopy(InlineOperands {
                        ins: [self.remapped_locals[&callee_output]],
                        outs: [call_output],
                    }));
                }
            } else {
                let relocated_operation =
                    program.operations.push(program.operations[old_operation]);
                if let Operation::InternalCall(relocated_call) = program.operations[old_operation] {
                    let tracked_callsite_operation = self.callsites[relocated_call.function]
                        .iter_mut()
                        .find(|callsite| **callsite == old_operation)
                        .expect("internal call should have a tracked callsite");
                    *tracked_callsite_operation = relocated_operation;
                    self.callsite_blocks
                        .remove(&old_operation)
                        .expect("tracked callsite should have a current block");
                    assert!(
                        self.callsite_blocks.insert(relocated_operation, callsite_block).is_none()
                    );
                }
            }
        }

        program.basic_blocks[callsite_block].operations =
            Span::new(new_operations_start, program.operations.next_idx());
    }

    fn inline_cfg_callsite(
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
        let mut remapped_blocks = HashMap::new();
        self.remapped_locals.clear();
        let mut callee_block_worklist = vec![callee_entry];
        let remapped_block_placeholder = BasicBlock {
            inputs: Span::EMPTY,
            outputs: Span::EMPTY,
            operations: Span::EMPTY,
            control: Control::LastOpTerminates,
        };

        while let Some(callee_block_id) = callee_block_worklist.pop() {
            let remapped_block_id = *remapped_blocks
                .entry(callee_block_id)
                .or_insert_with(|| program.basic_blocks.push(remapped_block_placeholder));

            let callee_block = program.basic_blocks[callee_block_id];

            let inputs_start = program.locals.next_idx();
            for input_idx in callee_block.inputs.iter() {
                let callee_input = program.locals[input_idx];
                let remapped_input = remap_local(program, &mut self.remapped_locals, callee_input);
                program.locals.push(remapped_input);
            }
            let inputs_end = program.locals.next_idx();

            let operations_start = program.operations.next_idx();
            for callee_operation in callee_block.operations.iter() {
                let mut remapped_operation = program.operations[callee_operation];
                remapped_operation.visit_data_mut(&mut OperationRemapper {
                    program,
                    remapped_locals: &mut self.remapped_locals,
                });
                program.operations.push(remapped_operation);
            }

            let outputs_start = program.locals.next_idx();
            for output_idx in callee_block.outputs.iter() {
                let callee_output = program.locals[output_idx];
                let remapped_output =
                    remap_local(program, &mut self.remapped_locals, callee_output);
                program.locals.push(remapped_output);
            }
            let outputs_end = program.locals.next_idx();

            let remapped_control = match callee_block.control {
                Control::LastOpTerminates => Control::LastOpTerminates,
                Control::InternalReturn => Control::ContinuesTo(join_block),
                Control::ContinuesTo(callee_target) => {
                    let remapped_target =
                        *remapped_blocks.entry(callee_target).or_insert_with(|| {
                            callee_block_worklist.push(callee_target);
                            program.basic_blocks.push(remapped_block_placeholder)
                        });
                    Control::ContinuesTo(remapped_target)
                }
                Control::Branches(branch) => {
                    let remapped_zero_target =
                        *remapped_blocks.entry(branch.zero_target).or_insert_with(|| {
                            callee_block_worklist.push(branch.zero_target);
                            program.basic_blocks.push(remapped_block_placeholder)
                        });
                    let remapped_non_zero_target =
                        *remapped_blocks.entry(branch.non_zero_target).or_insert_with(|| {
                            callee_block_worklist.push(branch.non_zero_target);
                            program.basic_blocks.push(remapped_block_placeholder)
                        });
                    Control::Branches(Branch {
                        condition: self.remapped_locals[&branch.condition],
                        non_zero_target: remapped_non_zero_target,
                        zero_target: remapped_zero_target,
                    })
                }
                Control::Switch(switch) => {
                    let callee_cases = program.cases[switch.cases];
                    let targets_start_id = program.cases_bb_ids.next_idx();
                    for callee_target_idx in callee_cases.target_indices().iter() {
                        let callee_target = program.cases_bb_ids[callee_target_idx];
                        let remapped_target =
                            *remapped_blocks.entry(callee_target).or_insert_with(|| {
                                callee_block_worklist.push(callee_target);
                                program.basic_blocks.push(remapped_block_placeholder)
                            });
                        program.cases_bb_ids.push(remapped_target);
                    }
                    let remapped_cases = program.cases.push(Cases {
                        values_start_id: callee_cases.values_start_id,
                        targets_start_id,
                        cases_count: callee_cases.cases_count,
                    });
                    Control::Switch(Switch {
                        condition: self.remapped_locals[&switch.condition],
                        fallback: switch.fallback.map(|callee_target| {
                            *remapped_blocks.entry(callee_target).or_insert_with(|| {
                                callee_block_worklist.push(callee_target);
                                program.basic_blocks.push(remapped_block_placeholder)
                            })
                        }),
                        cases: remapped_cases,
                    })
                }
            };

            program.basic_blocks[remapped_block_id] = BasicBlock {
                inputs: Span::new(inputs_start, inputs_end),
                outputs: Span::new(outputs_start, outputs_end),
                operations: Span::new(operations_start, program.operations.next_idx()),
                control: remapped_control,
            };
        }

        program.basic_blocks[callsite_block].control =
            Control::ContinuesTo(remapped_blocks[&callee_entry]);
    }
}

pub(crate) trait InlineHeuristic {
    fn should_inline(&mut self, program: &EthIRProgram, function_id: FunctionId) -> bool;
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
}

impl InlineHeuristic for DefaultHeuristic {
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

struct OperationRemapper<'a> {
    program: &'a mut EthIRProgram,
    remapped_locals: &'a mut HashMap<LocalId, LocalId>,
}

impl<'d> OpVisitorMut<'d, ()> for &mut OperationRemapper<'_> {
    fn visit_inline_operands_mut<const INS: usize, const OUTS: usize>(
        self,
        data: &'d mut sir_data::operation::InlineOperands<INS, OUTS>,
    ) {
        for local in data.ins.iter_mut().chain(data.outs.iter_mut()) {
            *local = remap_local(self.program, self.remapped_locals, *local);
        }
    }

    fn visit_allocated_ins_mut<const INS: usize, const OUTS: usize>(
        self,
        data: &'d mut sir_data::operation::AllocatedIns<INS, OUTS>,
    ) {
        let callee_inputs = data.get_inputs(self.program).to_vec();
        data.ins_start = self.program.locals.next_idx();
        for callee_input in callee_inputs {
            let remapped_input = remap_local(self.program, self.remapped_locals, callee_input);
            self.program.locals.push(remapped_input);
        }

        for output in &mut data.outs {
            *output = remap_local(self.program, self.remapped_locals, *output);
        }
    }

    fn visit_static_alloc_mut(self, data: &'d mut sir_data::operation::StaticAllocData) {
        data.ptr_out = remap_local(self.program, self.remapped_locals, data.ptr_out);
    }

    fn visit_memory_load_mut(self, data: &'d mut sir_data::operation::MemoryLoadData) {
        data.out = remap_local(self.program, self.remapped_locals, data.out);
        data.ptr = remap_local(self.program, self.remapped_locals, data.ptr);
    }

    fn visit_memory_store_mut(self, data: &'d mut sir_data::operation::MemoryStoreData) {
        for local in &mut data.ins {
            *local = remap_local(self.program, self.remapped_locals, *local);
        }
    }

    fn visit_set_small_const_mut(self, data: &'d mut sir_data::operation::SetSmallConstData) {
        data.sets = remap_local(self.program, self.remapped_locals, data.sets);
    }

    fn visit_set_large_const_mut(self, data: &'d mut sir_data::operation::SetLargeConstData) {
        data.sets = remap_local(self.program, self.remapped_locals, data.sets);
    }

    fn visit_set_data_offset_mut(self, data: &'d mut sir_data::operation::SetDataOffsetData) {
        data.sets = remap_local(self.program, self.remapped_locals, data.sets);
    }

    fn visit_icall_mut(self, data: &'d mut sir_data::operation::InternalCallData) {
        let callee_inputs = data.get_inputs(self.program).to_vec();
        let callee_outputs = data.get_outputs(self.program).to_vec();

        data.ins_start = self.program.locals.next_idx();
        for callee_input in callee_inputs {
            let remapped_input = remap_local(self.program, self.remapped_locals, callee_input);
            self.program.locals.push(remapped_input);
        }

        data.outs_start = self.program.locals.next_idx();
        for callee_output in callee_outputs {
            let remapped_output = remap_local(self.program, self.remapped_locals, callee_output);
            self.program.locals.push(remapped_output);
        }
    }

    fn visit_void_mut(self) {}
}

fn remap_local(
    program: &mut EthIRProgram,
    remapped_locals: &mut HashMap<LocalId, LocalId>,
    local: LocalId,
) -> LocalId {
    *remapped_locals.entry(local).or_insert_with(|| program.next_free_local_id.get_and_inc())
}

fn walk_call_graph(
    program: &EthIRProgram,
    function_id: FunctionId,
    function_states: &mut IndexVec<FunctionId, FunctionState>,
    postorder: &mut Vec<FunctionId>,
    callsites: &mut IndexVec<FunctionId, Vec<OperationIdx>>,
    callsite_blocks: &mut HashMap<OperationIdx, BasicBlockId>,
) {
    match function_states[function_id] {
        FunctionState::Complete => return,
        FunctionState::InProgress => {
            unreachable!("recursive calls should be rejected before inlining")
        }
        FunctionState::NotStarted => {}
    }

    function_states[function_id] = FunctionState::InProgress;

    let mut visited_blocks = DenseIndexSet::new();
    let mut worklist = vec![program.functions[function_id].entry()];

    while let Some(block) = worklist.pop() {
        if !visited_blocks.add(block) {
            continue;
        }

        for operation_id in program.basic_blocks[block].operations.iter() {
            if let Operation::InternalCall(call) = program.operations[operation_id] {
                callsites[call.function].push(operation_id);
                callsite_blocks.insert(operation_id, block);
                walk_call_graph(
                    program,
                    call.function,
                    function_states,
                    postorder,
                    callsites,
                    callsite_blocks,
                );
            }
        }

        worklist.extend(program.basic_blocks[block].control.iter_outgoing(program));
    }

    function_states[function_id] = FunctionState::Complete;
    postorder.push(function_id);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FunctionState {
    NotStarted,
    InProgress,
    Complete,
}

#[cfg(test)]
mod tests {
    use super::{DefaultHeuristic, Inliner};
    use crate::{AnalysesStore, Defragmenter, Legalizer, run_pass};
    use sir_data::{Operation, assert_ir_display};
    use sir_parser::{EmitConfig, parse_or_panic};

    fn inline(source: &str) -> sir_data::EthIRProgram {
        let mut program = parse_or_panic(source, EmitConfig::init_only());
        let store = AnalysesStore::default();
        run_pass(&mut Inliner::new(DefaultHeuristic::new(4)), &mut program, &store);
        Legalizer::default()
            .run(&program, &store)
            .unwrap_or_else(|err| panic!("legalization failed after inlining: {err}\n{program}"));
        program
    }

    #[test]
    fn test_linear_single_callsite() {
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

                @1 {
                    $2 = const 0x2
                    $4 = add $2 $2
                    $3 = copy $4
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_nested_linear_inlining_and_defragmentation() {
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

                @1 $3 -> $6 {
                    $4 = const 0x1
                    $9 = add $3 $4
                    $5 = copy $9
                    $6 = add $5 $3
                    iret
                }

                @2 {
                    $7 = const 0x5
                    $10 = const 0x1
                    $11 = add $7 $10
                    $12 = copy $11
                    $13 = add $12 $7
                    $8 = copy $13
                    stop
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
                @0 {
                    $0 = const 0x5
                    $1 = const 0x1
                    $2 = add $0 $1
                    $3 = copy $2
                    $4 = add $3 $0
                    $5 = copy $4
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_linear_inlining_remaps_allocated_operation_inputs() {
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

                @1 {
                    $4 = const 0x2
                    $5 = const 0x3
                    $6 = const 0x5
                    $8 = addmod $4 $5 $6
                    $7 = copy $8
                    stop
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

                @1 {
                    $5 = const 0x1
                    $7 = add $5 $5
                    $8 = add $7 $5
                    $9 = add $8 $5
                    $10 = add $9 $5
                    $6 = copy $10
                    stop
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

                @1 {
                    $4 = const 0x1
                    $10 = add $4 $4
                    $11 = add $10 $4
                    $12 = add $11 $4
                    $5 = copy $12
                    $7 = add $5 $5
                    $8 = add $7 $5
                    $9 = add $8 $5
                    $6 = copy $9
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_linear_repeated_callsites_in_same_block() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    x = const 3
                    y = icall @id x
                    z = icall @id y
                    stop
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
            Init: @1
            Functions:
                fn @0 -> entry @0  (outputs: 1)
                fn @1 -> entry @1  (outputs: 0)

            Basic Blocks:
                @0 $0 -> $0 {
                    iret
                }

                @1 {
                    $1 = const 0x3
                    $2 = copy $1
                    $3 = copy $2
                    stop
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
        run_pass(&mut Inliner::new(DefaultHeuristic::new(4)), &mut actual, &store);
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

                @1 {
                    $2 = const 0x1
                    $7 = add $2 $2
                    $3 = copy $7
                    stop
                }

                @2 {
                    $4 = const 0x2
                    $6 = add $4 $4
                    $5 = copy $6
                    stop
                }
            "#,
        );
    }

    #[test]
    fn test_cfg_repeated_callsites_in_same_block() {
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
    fn test_cfg_inlining_updates_moved_callsite_tracking() {
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

                @7 $8 {
                    $9 = copy $8
                    => $6 ? @5 : @6
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
            "#,
        );
    }

    #[test]
    fn test_cfg_inlining_handles_switch_loop() {
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
    fn test_cfg_inlining_preserves_static_allocation_identity() {
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
