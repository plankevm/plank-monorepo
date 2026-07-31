use hashbrown::HashMap;
use plank_core::{DenseIndexSet, IncIterable, IndexVec, Span, index_vec};
use sir_data::{
    BasicBlock, BasicBlockId, Branch, Cases, Control, EthIRProgram, FunctionId, LocalId, Operation,
    OperationIdx, Switch,
    operation::{InlineOperands, InternalCallData, OpVisitorMut},
};

use crate::{AnalysesStore, Pass};

#[derive(Debug, Clone, Default)]
pub(crate) struct Inliner {
    postorder: Vec<FunctionId>,
    callsites: IndexVec<FunctionId, Vec<OperationIdx>>,
    call_blocks: HashMap<OperationIdx, BasicBlockId>,
}

impl Pass for Inliner {
    fn run(&mut self, program: &mut EthIRProgram, _store: &AnalysesStore) {
        *self = Self::new(program);
        for function_id in self.postorder.clone() {
            self.inline_function(program, function_id);
        }
    }
}

impl Inliner {
    pub(crate) fn new(program: &EthIRProgram) -> Self {
        let mut callsites = index_vec![Vec::new(); program.functions.len()];
        let mut call_blocks = HashMap::new();
        let mut postorder = Vec::new();
        let mut function_states = index_vec![FunctionState::NotStarted; program.functions.len()];

        walk_call_graph(
            program,
            program.init_entry,
            &mut function_states,
            &mut postorder,
            &mut callsites,
            &mut call_blocks,
        );
        if let Some(main_entry) = program.main_entry {
            walk_call_graph(
                program,
                main_entry,
                &mut function_states,
                &mut postorder,
                &mut callsites,
                &mut call_blocks,
            );
        }

        Self { postorder, callsites, call_blocks }
    }

    fn inline_function(&mut self, program: &mut EthIRProgram, function_id: FunctionId) {
        let entry = program.functions[function_id].entry();
        let is_linear = matches!(program.basic_blocks[entry].control, Control::InternalReturn);
        while let Some(operation) = self.callsites[function_id].pop() {
            let block = self
                .call_blocks
                .remove(&operation)
                .expect("tracked callsite should have a current block");
            let Operation::InternalCall(call) = program.operations[operation] else {
                unreachable!("tracked callsite should point to an internal call")
            };
            assert_eq!(call.function, function_id);
            if is_linear {
                self.inline_linear_callsite(program, block, operation, call);
            } else {
                self.inline_cfg_callsite(program, block, operation, call);
            }
        }
    }

    fn inline_linear_callsite(
        &mut self,
        program: &mut EthIRProgram,
        caller_id: BasicBlockId,
        operation: OperationIdx,
        call: InternalCallData,
    ) {
        let caller_operations = program.basic_blocks[caller_id].operations;
        let new_operations_start = program.operations.next_idx();

        for old_operation in caller_operations.iter() {
            if old_operation == operation {
                let callee_entry = program.functions[call.function].entry();
                let callee_operations = program.basic_blocks[callee_entry].operations;
                let callee_inputs = program.block(callee_entry).inputs();
                let call_inputs = call.get_inputs(program);
                let mut callee_locals = HashMap::new();
                for i in 0..callee_inputs.len() {
                    callee_locals.insert(callee_inputs[i], call_inputs[i]);
                }

                for callee_operation in callee_operations.iter() {
                    let mut remapped = program.operations[callee_operation];
                    remapped.visit_data_mut(&mut OperationRemapper {
                        program,
                        locals: &mut callee_locals,
                    });
                    program.operations.push(remapped);
                }

                for i in 0..program.block(callee_entry).outputs().len() {
                    let return_id = program.block(callee_entry).outputs()[i];
                    let call_output = call.get_outputs(program)[i];
                    program.operations.push(Operation::SetCopy(InlineOperands {
                        ins: [callee_locals[&return_id]],
                        outs: [call_output],
                    }));
                }
            } else {
                let new_operation = program.operations.push(program.operations[old_operation]);
                if let Operation::InternalCall(call) = program.operations[old_operation] {
                    let callsite = self.callsites[call.function]
                        .iter_mut()
                        .find(|callsite| **callsite == old_operation)
                        .expect("internal call should have a tracked callsite");
                    *callsite = new_operation;
                    self.call_blocks
                        .remove(&old_operation)
                        .expect("tracked callsite should have a current block");
                    assert!(self.call_blocks.insert(new_operation, caller_id).is_none());
                }
            }
        }

        program.basic_blocks[caller_id].operations =
            Span::new(new_operations_start, program.operations.next_idx());
    }

    fn inline_cfg_callsite(
        &mut self,
        program: &mut EthIRProgram,
        caller_id: BasicBlockId,
        operation: OperationIdx,
        call: InternalCallData,
    ) {
        let caller = program.basic_blocks[caller_id];

        program.basic_blocks[caller_id].outputs = Span::new(call.ins_start, call.outs_start);
        program.basic_blocks[caller_id].operations = Span::new(caller.operations.start, operation);

        let return_block = program.basic_blocks.push(BasicBlock {
            inputs: Span::new(
                call.outs_start,
                call.outs_start + program.functions[call.function].get_outputs(),
            ),
            outputs: caller.outputs,
            operations: Span::new(operation + 1, caller.operations.end),
            control: caller.control,
        });

        for moved_operation in program.basic_blocks[return_block].operations.iter() {
            if let Operation::InternalCall(call) = program.operations[moved_operation] {
                let previous_block = self
                    .call_blocks
                    .insert(moved_operation, return_block)
                    .expect("internal call in moved suffix should have a tracked callsite block");
                assert_eq!(previous_block, caller_id);
                assert!(
                    self.callsites[call.function].contains(&moved_operation),
                    "internal call in moved suffix should have a tracked callsite"
                );
            }
        }

        let callee_entry = program.functions[call.function].entry();
        let mut block_map = HashMap::new();
        let mut callee_locals = HashMap::new();
        let mut block_worklist = vec![callee_entry];
        let block_placeholder = BasicBlock {
            inputs: Span::EMPTY,
            outputs: Span::EMPTY,
            operations: Span::EMPTY,
            control: Control::LastOpTerminates,
        };

        while let Some(block) = block_worklist.pop() {
            let remapped_block_id = *block_map
                .entry(block)
                .or_insert_with(|| program.basic_blocks.push(block_placeholder));

            let source = program.basic_blocks[block];

            let inputs_start = program.locals.next_idx();
            for input in source.inputs.iter() {
                let source_input = program.locals[input];
                let remapped_input = OperationRemapper { program, locals: &mut callee_locals }
                    .remap_local(source_input);
                program.locals.push(remapped_input);
            }
            let inputs_end = program.locals.next_idx();

            let operations_start = program.operations.next_idx();
            for op_id in source.operations.iter() {
                let mut remapped = program.operations[op_id];
                remapped
                    .visit_data_mut(&mut OperationRemapper { program, locals: &mut callee_locals });
                program.operations.push(remapped);
            }

            let outputs_start = program.locals.next_idx();
            for output in source.outputs.iter() {
                let source_output = program.locals[output];
                let remapped_output = OperationRemapper { program, locals: &mut callee_locals }
                    .remap_local(source_output);
                program.locals.push(remapped_output);
            }
            let outputs_end = program.locals.next_idx();

            let control = match source.control {
                Control::LastOpTerminates => Control::LastOpTerminates,
                Control::InternalReturn => Control::ContinuesTo(return_block),
                Control::ContinuesTo(target) => {
                    let remapped_target = *block_map.entry(target).or_insert_with(|| {
                        block_worklist.push(target);
                        program.basic_blocks.push(block_placeholder)
                    });
                    Control::ContinuesTo(remapped_target)
                }
                Control::Branches(branch) => {
                    let zero_target = *block_map.entry(branch.zero_target).or_insert_with(|| {
                        block_worklist.push(branch.zero_target);
                        program.basic_blocks.push(block_placeholder)
                    });
                    let non_zero_target =
                        *block_map.entry(branch.non_zero_target).or_insert_with(|| {
                            block_worklist.push(branch.non_zero_target);
                            program.basic_blocks.push(block_placeholder)
                        });
                    Control::Branches(Branch {
                        condition: callee_locals[&branch.condition],
                        non_zero_target,
                        zero_target,
                    })
                }
                Control::Switch(switch) => {
                    let cases = program.cases[switch.cases];
                    let targets_start_id = program.cases_bb_ids.next_idx();
                    for source_target_idx in cases.target_indices().iter() {
                        let source_target = program.cases_bb_ids[source_target_idx];
                        let remapped_target =
                            *block_map.entry(source_target).or_insert_with(|| {
                                block_worklist.push(source_target);
                                program.basic_blocks.push(block_placeholder)
                            });
                        program.cases_bb_ids.push(remapped_target);
                    }
                    let cases = program.cases.push(Cases {
                        values_start_id: cases.values_start_id,
                        targets_start_id,
                        cases_count: cases.cases_count,
                    });
                    Control::Switch(Switch {
                        condition: callee_locals[&switch.condition],
                        fallback: switch.fallback.map(|target| {
                            *block_map.entry(target).or_insert_with(|| {
                                block_worklist.push(target);
                                program.basic_blocks.push(block_placeholder)
                            })
                        }),
                        cases,
                    })
                }
            };

            program.basic_blocks[remapped_block_id] = BasicBlock {
                inputs: Span::new(inputs_start, inputs_end),
                outputs: Span::new(outputs_start, outputs_end),
                operations: Span::new(operations_start, program.operations.next_idx()),
                control,
            };
        }

        program.basic_blocks[caller_id].control = Control::ContinuesTo(block_map[&callee_entry]);
    }
}

struct OperationRemapper<'a> {
    program: &'a mut EthIRProgram,
    locals: &'a mut HashMap<LocalId, LocalId>,
}

impl OperationRemapper<'_> {
    fn remap_local(&mut self, local: LocalId) -> LocalId {
        *self.locals.entry(local).or_insert_with(|| self.program.next_free_local_id.get_and_inc())
    }
}

impl<'d> OpVisitorMut<'d, ()> for &mut OperationRemapper<'_> {
    fn visit_inline_operands_mut<const INS: usize, const OUTS: usize>(
        self,
        data: &'d mut sir_data::operation::InlineOperands<INS, OUTS>,
    ) {
        for local in data.ins.iter_mut().chain(data.outs.iter_mut()) {
            *local = self.remap_local(*local);
        }
    }

    fn visit_allocated_ins_mut<const INS: usize, const OUTS: usize>(
        self,
        data: &'d mut sir_data::operation::AllocatedIns<INS, OUTS>,
    ) {
        let inputs = data.get_inputs(self.program).to_vec();
        data.ins_start = self.program.locals.next_idx();
        for input in inputs {
            let input = self.remap_local(input);
            self.program.locals.push(input);
        }

        for output in &mut data.outs {
            *output = self.remap_local(*output);
        }
    }

    fn visit_static_alloc_mut(self, data: &'d mut sir_data::operation::StaticAllocData) {
        data.ptr_out = self.remap_local(data.ptr_out);
    }

    fn visit_memory_load_mut(self, data: &'d mut sir_data::operation::MemoryLoadData) {
        data.out = self.remap_local(data.out);
        data.ptr = self.remap_local(data.ptr);
    }

    fn visit_memory_store_mut(self, data: &'d mut sir_data::operation::MemoryStoreData) {
        for local in &mut data.ins {
            *local = self.remap_local(*local);
        }
    }

    fn visit_set_small_const_mut(self, data: &'d mut sir_data::operation::SetSmallConstData) {
        data.sets = self.remap_local(data.sets);
    }

    fn visit_set_large_const_mut(self, data: &'d mut sir_data::operation::SetLargeConstData) {
        data.sets = self.remap_local(data.sets);
    }

    fn visit_set_data_offset_mut(self, data: &'d mut sir_data::operation::SetDataOffsetData) {
        data.sets = self.remap_local(data.sets);
    }

    fn visit_icall_mut(self, data: &'d mut sir_data::operation::InternalCallData) {
        let inputs = data.get_inputs(self.program).to_vec();
        let outputs = data.get_outputs(self.program).to_vec();

        data.ins_start = self.program.locals.next_idx();
        for input in inputs {
            let input = self.remap_local(input);
            self.program.locals.push(input);
        }

        data.outs_start = self.program.locals.next_idx();
        for output in outputs {
            let output = self.remap_local(output);
            self.program.locals.push(output);
        }
    }

    fn visit_void_mut(self) {}
}

fn walk_call_graph(
    program: &EthIRProgram,
    function_id: FunctionId,
    function_states: &mut IndexVec<FunctionId, FunctionState>,
    postorder: &mut Vec<FunctionId>,
    callsites: &mut IndexVec<FunctionId, Vec<OperationIdx>>,
    call_blocks: &mut HashMap<OperationIdx, BasicBlockId>,
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

        for op_id in program.basic_blocks[block].operations.iter() {
            if let Operation::InternalCall(data) = program.operations[op_id] {
                callsites[data.function].push(op_id);
                call_blocks.insert(op_id, block);
                walk_call_graph(
                    program,
                    data.function,
                    function_states,
                    postorder,
                    callsites,
                    call_blocks,
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
    use super::Inliner;
    use crate::{AnalysesStore, Legalizer, run_pass};
    use sir_data::{Operation, assert_ir_display};
    use sir_parser::{EmitConfig, parse_or_panic};

    fn inline(source: &str) -> sir_data::EthIRProgram {
        let mut program = parse_or_panic(source, EmitConfig::init_only());
        let store = AnalysesStore::default();
        run_pass(&mut Inliner::default(), &mut program, &store);
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
    fn test_linear_nested_calls() {
        let actual = inline(
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
    }

    #[test]
    fn test_linear_allocated_inputs() {
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
    fn test_cfg_inline_updates_moved_callsite() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    cond = const 1
                    x = const 7
                    y = icall @choose cond x
                    z = icall @id y
                    stop
                }

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
                    => @6
                }

                @5 $8 {
                    $9 = copy $8
                    stop
                }

                @6 $10 $11 -> $11 {
                    => $10 ? @8 : @7
                }

                @7 $13 -> $14 {
                    $14 = const 0x0
                    => @5
                }

                @8 $12 -> $12 {
                    => @5
                }
            "#,
        );
    }

    #[test]
    fn test_cfg_inline_switch_loop() {
        let actual = inline(
            r#"
            fn init:
                entry {
                    selector = const 1
                    x = const 3
                    y = icall @countdown selector x
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
                done _ done_x -> done_x {
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

                @2 $6 $7 -> $7 {
                    iret
                }

                @3 -> $8 $9 {
                    $8 = const 0x1
                    $9 = const 0x3
                    => @5
                }

                @4 $10 {
                    stop
                }

                @5 $11 $12 -> $11 $12 {
                    => @6
                }

                @6 $13 $14 -> $13 $16 {
                    $15 = const 0x1
                    $16 = sub $14 $15
                    switch $14 {
                        0x0 => @7,
                        else => @6
                    }

                }

                @7 $17 $18 -> $18 {
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
