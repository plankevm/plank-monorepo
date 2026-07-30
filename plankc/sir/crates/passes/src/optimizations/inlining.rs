use hashbrown::HashMap;
use plank_core::{DenseIndexSet, IncIterable, IndexVec, Span, index_vec};
use sir_data::{
    BasicBlockId, Control, EthIRProgram, FunctionId, LocalId, Operation, OperationIdx,
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
    fn run(&mut self, _program: &mut EthIRProgram, _store: &AnalysesStore) {
        todo!("implement inlining pass")
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
                self.inline_cfg_callsite(program, function_id, block, operation);
            }
        }
    }

    fn inline_linear_callsite(
        &mut self,
        program: &mut EthIRProgram,
        block: BasicBlockId,
        operation: OperationIdx,
        call: InternalCallData,
    ) {
        let new_operations_start = program.operations.next_idx();

        for old_operation in program.basic_blocks[block].operations.iter() {
            if old_operation == operation {
                let callee_entry = program.functions[call.function].entry();
                let callee_inputs = program.block(callee_entry).inputs();
                let call_inputs = call.get_inputs(program);
                let mut callee_locals = HashMap::new();
                for i in 0..callee_inputs.len() {
                    callee_locals.insert(callee_inputs[i], call_inputs[i]);
                }

                for callee_operation in program.basic_blocks[callee_entry].operations.iter() {
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
                    self.call_blocks.insert(new_operation, block);
                }
            }
        }

        program.basic_blocks[block].operations =
            Span::new(new_operations_start, program.operations.next_idx());
    }

    fn inline_cfg_callsite(
        &mut self,
        _program: &mut EthIRProgram,
        _function_id: FunctionId,
        _block: BasicBlockId,
        _operation: OperationIdx,
    ) {
        // Split the caller at the `icall`, move the suffix to a continuation block, and clone the
        // callee CFG between the prefix and continuation.
        // Update tracked callsite blocks for calls moved to the continuation.
        // Map call arguments to cloned callee entry inputs and callee returns to continuation inputs.
        // Rewrite every cloned `iret` to jump to the continuation.
        // Leave the original callee and orphaned IR behind for defragmentation.
        todo!("implement multi-block callsite inlining")
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
        data.alloc_id = self.program.next_static_alloc_id.get_and_inc();
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
