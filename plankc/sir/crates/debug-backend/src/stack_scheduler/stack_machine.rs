/*
 * Stack Calling Convention
 *
 * Block inputs and outputs follow a top-of-stack-first convention:
 * - The first declared output is on top of the stack.
 * - The last declared output is at the bottom.
 * - At block entry, the first declared input is on top.
 *
 * Example: outputs [e, g, c] results in stack (bottom to top): c, g, e
 * The next block's inputs [x, y, z] maps: x=e (top), y=g, z=c (bottom)
 */

use plank_core::DenseIndexSet;
use sir_assembler::{AsmReference, Assembler, op};
use sir_data::{
    BasicBlockId, ControlView, FunctionId, InlineOperands, LocalId, Operation, OperationView,
};

use crate::{
    MarkMap, TranslationContext, TranslationPhase, operations::op_kind_to_direct_op,
    static_memory_layout::StaticMemoryLayout,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackEntry {
    Local(LocalId),
    Intermediate,
}

pub(crate) struct StackMachine {
    stack: Vec<StackEntry>,
    in_memory: DenseIndexSet<LocalId>,
    remap_buf: Vec<LocalId>,
    _spill_threshold: u8,
}

impl StackMachine {
    pub fn new(_spill_threshold: u8) -> Self {
        Self {
            stack: Vec::new(),
            in_memory: DenseIndexSet::new(),
            remap_buf: Vec::new(),
            _spill_threshold,
        }
    }

    fn set_stack_from_layout(
        &mut self,
        bb_id: BasicBlockId,
        block_inputs: &[LocalId],
        ctx: &TranslationContext,
    ) {
        self.stack.clear();
        let Some(in_group) = ctx.bundling.get_in_group(bb_id) else {
            return;
        };
        let preds = ctx.predecessors.of(bb_id);
        debug_assert!(!preds.is_empty(), "block with in-group must have predecessors");
        let pred_outputs = ctx.ir.block(preds[0]).outputs();
        for local in &ctx.group_layouts[in_group] {
            let remapped = pred_outputs
                .iter()
                .position(|&out| out == *local)
                .map(|pos| block_inputs[pos])
                .unwrap_or(*local);
            self.stack.push(StackEntry::Local(remapped));
        }
    }

    /// DUPs `local` to the top of the stack, or loads it from memory if spilled.
    /// Dead copies remain until `arrange_stack_to_layout` cleans them up at block end.
    fn prepare_input(&mut self, local: LocalId, asm: &mut Assembler, ctx: &TranslationContext) {
        let depth = self.stack.iter().rev().position(|&e| e == StackEntry::Local(local));
        match depth {
            Some(d) if d < 16 => {
                asm.push_op_byte(op::dup_n(d as u8 + 1));
                self.stack.push(StackEntry::Local(local));
            }
            _ => {
                assert!(
                    self.in_memory.contains(local),
                    "local {local} is not reachable on stack or in memory"
                );
                ctx.memory_layout.emit_local_load(asm, local);
                self.stack.push(StackEntry::Local(local));
            }
        }
    }

    /// Generic handler for operations with a direct EVM opcode.
    /// Prepares all inputs on the stack (in EVM operand order), emits the
    /// opcode, then updates the symbolic stack with the outputs.
    fn emit_standard_op(
        &mut self,
        op_view: OperationView,
        asm: &mut Assembler,
        ctx: &TranslationContext,
    ) {
        let inputs = op_view.inputs();
        for input in inputs.iter().rev() {
            self.prepare_input(*input, asm, ctx);
        }

        let evm_op =
            op_kind_to_direct_op(op_view.op().kind()).expect("standard op has direct EVM mapping");
        asm.push_op_byte(evm_op);

        self.stack.truncate(self.stack.len() - inputs.len());

        for output in op_view.outputs() {
            self.stack.push(StackEntry::Local(*output));
        }
    }

    pub fn dispatch_block(
        &mut self,
        func: FunctionId,
        bb_id: BasicBlockId,
        asm: &mut Assembler,
        ctx: &mut TranslationContext,
    ) {
        let block = ctx.ir.block(bb_id);
        self.set_stack_from_layout(bb_id, block.inputs(), ctx);

        for op_view in block.operations() {
            self.dispatch_operation(op_view, asm, ctx);
        }

        let control = block.control();
        match control {
            ControlView::LastOpTerminates => {}
            ControlView::InternalReturn => {
                Self::arrange_stack_to_layout(&mut self.stack, block.outputs(), None, asm);
            }
            ControlView::ContinuesTo(_) | ControlView::Branches { .. } | ControlView::Switch(_) => {
                let condition = match control {
                    ControlView::Branches { condition, .. } => Some(condition),
                    ControlView::Switch(switch) => Some(switch.condition()),
                    _ => None,
                };
                let out_layout = Self::get_out_layout(&self.stack, bb_id, ctx, &mut self.remap_buf);
                Self::arrange_stack_to_layout(&mut self.stack, out_layout, condition, asm);
            }
        }

        self.emit_control_flow(control, func, asm, ctx.mark_map, ctx.memory_layout);
    }

    fn get_out_layout<'a>(
        stack: &[StackEntry],
        bb_id: BasicBlockId,
        ctx: &'a TranslationContext,
        remap_buf: &'a mut Vec<LocalId>,
    ) -> &'a [LocalId] {
        let group_id = ctx
            .bundling
            .get_out_group(bb_id)
            .expect("block with successors must have an out-group");
        let group_layout = &ctx.group_layouts[group_id];
        let block_outputs = ctx.ir.block(bb_id).outputs();

        if block_outputs.is_empty() {
            return group_layout;
        }

        // Remap group layout entries that belong to the reference block's outputs
        // to this block's outputs at the same position.
        remap_buf.clear();
        let mut output_idx = 0;
        for entry in group_layout {
            if stack.contains(&StackEntry::Local(*entry)) {
                remap_buf.push(*entry);
            } else {
                remap_buf.push(block_outputs[output_idx]);
                output_idx += 1;
            }
        }
        remap_buf
    }

    /// Arranges the stack so layout elements are at positions 0..layout.len() and the
    /// condition (if present) is on top. All other values are POPped.
    fn arrange_stack_to_layout(
        stack: &mut Vec<StackEntry>,
        layout: &[LocalId],
        condition: Option<LocalId>,
        asm: &mut Assembler,
    ) {
        let condition_in_layout = condition.is_some_and(|c| layout.contains(&c));
        let target_stack_depth =
            layout.len() + condition.is_some() as usize - condition_in_layout as usize;

        // Target position for a stack entry, or None if junk.
        let target_of = |entry: StackEntry| match entry {
            StackEntry::Intermediate => None,
            StackEntry::Local(local) => layout.iter().position(|l| *l == local).or_else(|| {
                (!condition_in_layout && condition == Some(local)).then_some(layout.len())
            }),
        };

        // Process from the top: POP junk, SWAP layout/condition to target position.
        // Each SWAP follows a permutation cycle, surfacing junk to be POPped.
        while let Some(&top_entry) = stack.last() {
            if stack.len() <= target_stack_depth {
                break;
            }
            match target_of(top_entry) {
                None => {
                    stack.pop();
                    asm.push_op_byte(op::POP);
                }
                Some(target) => {
                    let top = stack.len() - 1;
                    asm.push_op_byte(op::swap_n((top - target) as u8));
                    stack.swap(top, target);
                }
            }
        }

        // Fix remaining out-of-position elements that were already below the junk.
        for i in 0..target_stack_depth {
            let target = target_of(stack[i]).expect("all junk already popped");
            if target == i {
                continue;
            }
            let top = target_stack_depth - 1;
            if i != top {
                asm.push_op_byte(op::swap_n((top - i) as u8));
                stack.swap(top, i);
            }
            if target != top {
                asm.push_op_byte(op::swap_n((top - target) as u8));
                stack.swap(top, target);
            }
        }

        // If the condition is in the layout, DUP it so JUMPI consumes the copy.
        if let Some(cond) = condition.filter(|_| condition_in_layout) {
            let depth = stack
                .iter()
                .rev()
                .position(|&e| e == StackEntry::Local(cond))
                .expect("condition must be on the stack");
            asm.push_op_byte(op::dup_n((depth + 1) as u8));
            stack.push(StackEntry::Local(cond));
        }
    }

    /// Emits control flow for a block. For Branches/Switch, assumes the
    /// condition is already on top of the stack.
    fn emit_control_flow(
        &mut self,
        control: ControlView,
        func: FunctionId,
        asm: &mut Assembler,
        mark_map: &MarkMap,
        memory_layout: &StaticMemoryLayout,
    ) {
        match control {
            ControlView::LastOpTerminates => {}
            ControlView::InternalReturn => {
                let return_dest_loc = memory_layout.get_return_dest_store(func);
                asm.push_minimal_u32(return_dest_loc);
                asm.push_op_byte(op::MLOAD);
                asm.push_op_byte(op::JUMP);
            }
            ControlView::ContinuesTo(to) => {
                mark_map.emit_code_offset_push(asm, mark_map.get_bb_mark(to));
                asm.push_op_byte(op::JUMP);
            }
            ControlView::Branches { non_zero_target, zero_target, .. } => {
                self.stack.pop(); // condition
                mark_map.emit_code_offset_push(asm, mark_map.get_bb_mark(non_zero_target));
                asm.push_op_byte(op::JUMPI);
                mark_map.emit_code_offset_push(asm, mark_map.get_bb_mark(zero_target));
                asm.push_op_byte(op::JUMP);
            }
            ControlView::Switch(switch) => {
                self.stack.pop(); // condition
                asm.push_minimal_u32(memory_layout.switch_store);
                asm.push_op_byte(op::MSTORE);

                for (value, bb) in switch.cases() {
                    asm.push_minimal_u32(memory_layout.switch_store);
                    asm.push_op_byte(op::MLOAD);
                    asm.push_minimal_u256(value);
                    asm.push_op_byte(op::EQ);
                    mark_map.emit_code_offset_push(asm, mark_map.get_bb_mark(bb));
                    asm.push_op_byte(op::JUMPI);
                }

                if let Some(fallback) = switch.fallback() {
                    mark_map.emit_code_offset_push(asm, mark_map.get_bb_mark(fallback));
                    asm.push_op_byte(op::JUMP);
                } else {
                    asm.emit_undefined_behavior_error();
                }
            }
        }
    }

    fn dispatch_operation(
        &mut self,
        op_view: OperationView,
        asm: &mut Assembler,
        ctx: &mut TranslationContext,
    ) {
        let op = op_view.op();
        match op {
            // Constants: 0 in, 1 out, push a value
            Operation::SetSmallConst(data) => {
                asm.push_minimal_u32(data.value);
                self.stack.push(StackEntry::Local(data.sets));
            }
            Operation::SetLargeConst(data) => {
                asm.push_minimal_u256(ctx.ir.large_consts[data.value]);
                self.stack.push(StackEntry::Local(data.sets));
            }
            Operation::SetDataOffset(data) => {
                let data_mark = ctx.mark_map.get_data_mark(data.segment_id);
                ctx.mark_map.emit_code_offset_push(asm, data_mark);
                self.stack.push(StackEntry::Local(data.sets));
            }
            Operation::RuntimeStartOffset(data) => {
                debug_assert!(
                    ctx.mark_map.phase() == TranslationPhase::Init,
                    "unexpected runtime_start_offset in run code"
                );
                asm.push_reference(AsmReference::new_direct(ctx.mark_map.runtime_start));
                self.stack.push(StackEntry::Local(data.outs[0]));
            }
            Operation::InitEndOffset(data) => {
                debug_assert!(
                    ctx.mark_map.phase() == TranslationPhase::Init,
                    "unexpected init_end_offset in run code"
                );
                asm.push_reference(AsmReference::new_direct(ctx.mark_map.initcode_end));
                self.stack.push(StackEntry::Local(data.outs[0]));
            }
            Operation::RuntimeLength(data) => {
                asm.push_reference(AsmReference::new_delta(
                    ctx.mark_map.runtime_start,
                    ctx.mark_map.initcode_end,
                ));
                self.stack.push(StackEntry::Local(data.outs[0]));
            }

            // Memory: allocation and memory I/O
            Operation::AcquireFreePointer(InlineOperands { ins: [], outs: [dst] }) => {
                ctx.memory_layout.emit_free_ptr_load(asm);
                self.stack.push(StackEntry::Local(dst));
            }

            Operation::MemoryLoad(data) => {
                let load_size = data.size as u32;
                self.prepare_input(data.ptr, asm, ctx);
                // evm: [ptr]                                   symbolic: [ptr]
                self.stack.pop();
                asm.push_op_byte(op::MLOAD);
                // evm: [raw_word]                              symbolic: []
                asm.push_minimal_u32(256 - load_size * 8);
                asm.push_op_byte(op::SHR);
                self.stack.push(StackEntry::Local(data.out));
                // evm: [value]                                 symbolic: [out]
            }
            Operation::MemoryStore(data) => {
                let load_size = data.size as u32;
                let shift_to_clean_word = load_size * 8;
                self.prepare_input(data.ptr(), asm, ctx);
                // evm: [ptr]                                   symbolic: [ptr]
                asm.push_op_byte(op::DUP1);
                // evm: [ptr, ptr]                              symbolic: [ptr]
                asm.push_op_byte(op::MLOAD);
                // evm: [current_word, ptr]                     symbolic: [ptr]
                asm.push_minimal_u32(shift_to_clean_word);
                asm.push_op_byte(op::SHL);
                // evm: [current_word << shift, ptr]            symbolic: [ptr]
                asm.push_minimal_u32(shift_to_clean_word);
                asm.push_op_byte(op::SHR);
                self.stack.push(StackEntry::Intermediate);
                // evm: [cleaned_word, ptr]                     symbolic: [Intermediate, ptr]
                self.prepare_input(data.value(), asm, ctx);
                // evm: [value, cleaned_word, ptr]              symbolic: [value, Intermediate, ptr]
                asm.push_minimal_u32(256 - load_size * 8);
                asm.push_op_byte(op::SHL);
                // evm: [shifted_value, cleaned_word, ptr]      symbolic: [value, Intermediate, ptr]
                asm.push_op_byte(op::OR);
                self.stack.pop(); // value
                self.stack.pop(); // Intermediate
                self.stack.push(StackEntry::Intermediate); // updated_word
                // evm: [updated_word, ptr]                     symbolic: [Intermediate, ptr]
                asm.push_op_byte(op::SWAP1);
                // evm: [ptr, updated_word]                     symbolic: [Intermediate, ptr]
                asm.push_op_byte(op::MSTORE);
                self.stack.pop(); // Intermediate
                self.stack.pop(); // ptr
                // evm: []                                      symbolic: []
            }

            Operation::DynamicAllocZeroed(InlineOperands { ins: [size], outs: [ptr_out] })
            | Operation::DynamicAllocAnyBytes(InlineOperands { ins: [size], outs: [ptr_out] }) => {
                ctx.memory_layout.emit_free_ptr_load(asm);
                self.stack.push(StackEntry::Intermediate);
                // evm: [free_ptr]                                    symbolic: [Intermediate]
                asm.push_op_byte(op::DUP1);
                self.stack.push(StackEntry::Intermediate);
                // evm: [free_ptr, free_ptr]                          symbolic: [Intermediate, Intermediate]
                self.prepare_input(size, asm, ctx);
                // evm: [size, free_ptr, free_ptr]                    symbolic: [size, Intermediate, Intermediate]
                asm.push_op_byte(op::DUP1);
                asm.push_op_byte(op::CALLDATASIZE);
                asm.push_op_byte(op::DUP4);
                asm.push_op_byte(op::CALLDATACOPY);
                // evm: [size, free_ptr, free_ptr]                    symbolic: [size, Intermediate, Intermediate]
                asm.push_op_byte(op::ADD);
                // evm: [free_ptr', free_ptr]                         symbolic: [size, Intermediate, Intermediate]
                asm.push_minimal_u32(ctx.memory_layout.free_pointer);
                asm.push_op_byte(op::MSTORE);
                // evm: [free_ptr]                                    symbolic: [size, Intermediate, Intermediate]
                self.stack.pop(); // size
                self.stack.pop(); // Intermediate
                self.stack.pop(); // Intermediate
                self.stack.push(StackEntry::Local(ptr_out));
                // evm: [free_ptr]                                    symbolic: [ptr_out]
            }
            Operation::StaticAllocZeroed(data)
            | Operation::StaticAllocAnyBytes(data) => {
                ctx.memory_layout.emit_free_ptr_load(asm);
                // evm: [free_ptr]
                asm.push_op_byte(op::DUP1);
                // evm: [free_ptr, free_ptr]
                asm.push_minimal_u32(data.size);
                // evm: [size, free_ptr, free_ptr]
                asm.push_op_byte(op::DUP1);
                asm.push_op_byte(op::CALLDATASIZE);
                asm.push_op_byte(op::DUP4);
                asm.push_op_byte(op::CALLDATACOPY);
                // evm: [size, free_ptr, free_ptr]
                asm.push_op_byte(op::ADD);
                // evm: [free_ptr', free_ptr]
                asm.push_minimal_u32(ctx.memory_layout.free_pointer);
                asm.push_op_byte(op::MSTORE);
                // evm: [free_ptr]
                self.stack.push(StackEntry::Local(data.ptr_out));
                // evm: [free_ptr]                                    symbolic: [ptr_out]
            }

            Operation::InternalCall(data) => {
                let inputs = data.get_inputs(ctx.ir);
                let outputs = data.get_outputs(ctx.ir);

                // Push call args onto the stack
                for input in inputs.iter().rev() {
                    self.prepare_input(*input, asm, ctx);
                }
                // evm: [arg1, ..., argN]                      symbolic: [arg1, ..., argN]

                // Store return address to memory
                let return_mark = ctx.mark_map.allocate_mark();
                let return_store_loc = ctx.memory_layout.get_return_dest_store(data.function);
                ctx.mark_map.emit_code_offset_push(asm, return_mark);
                asm.push_minimal_u32(return_store_loc);
                asm.push_op_byte(op::MSTORE);
                // evm: [arg1, ..., argN]                      symbolic: [arg1, ..., argN]

                // Jump to callee entry
                let func_entry_bb = ctx.ir.function(data.function).entry().id();
                let func_entry_bb_mark = ctx.mark_map.get_bb_mark(func_entry_bb);
                ctx.mark_map.emit_code_offset_push(asm, func_entry_bb_mark);
                asm.push_op_byte(op::JUMP);
                // evm: [arg1, ..., argN]                      symbolic: [arg1, ..., argN]

                // Return lands here
                asm.push_mark(return_mark);
                asm.push_op_byte(op::JUMPDEST);
                // evm: [out1, ..., outM]                      symbolic: [arg1, ..., argN]

                // Update symbolic stack: pop args, push outputs
                for _ in inputs {
                    self.stack.pop();
                }
                for output in outputs {
                    self.stack.push(StackEntry::Local(*output));
                }
                // evm: [out1, ..., outM]                      symbolic: [out1, ..., outM]

                // Enqueue callee for translation
                ctx.bbs_to_be_translated.push((data.function, func_entry_bb));
            }

            Operation::SetCopy(InlineOperands { ins: [src], outs: [dst] }) => {
                self.prepare_input(src, asm, ctx);
                self.stack.pop();
                self.stack.push(StackEntry::Local(dst));
            }

            Operation::Noop(_) => {}

            // Nullary
            Operation::Address(_)
            | Operation::Origin(_)
            | Operation::Caller(_)
            | Operation::CallValue(_)
            | Operation::CallDataSize(_)
            | Operation::CodeSize(_)
            | Operation::GasPrice(_)
            | Operation::ReturnDataSize(_)
            | Operation::Gas(_)
            | Operation::Coinbase(_)
            | Operation::Timestamp(_)
            | Operation::Number(_)
            | Operation::Difficulty(_)
            | Operation::GasLimit(_)
            | Operation::ChainId(_)
            | Operation::SelfBalance(_)
            | Operation::BaseFee(_)
            | Operation::BlobBaseFee(_)
            // Unary
            | Operation::IsZero(_)
            | Operation::Not(_)
            | Operation::Balance(_)
            | Operation::CallDataLoad(_)
            | Operation::ExtCodeSize(_)
            | Operation::ExtCodeHash(_)
            | Operation::BlockHash(_)
            | Operation::SLoad(_)
            | Operation::TLoad(_)
            | Operation::BlobHash(_)
            // Binary
            | Operation::Add(_)
            | Operation::Mul(_)
            | Operation::Sub(_)
            | Operation::Div(_)
            | Operation::SDiv(_)
            | Operation::Mod(_)
            | Operation::SMod(_)
            | Operation::Exp(_)
            | Operation::SignExtend(_)
            | Operation::Lt(_)
            | Operation::Gt(_)
            | Operation::SLt(_)
            | Operation::SGt(_)
            | Operation::Eq(_)
            | Operation::And(_)
            | Operation::Or(_)
            | Operation::Xor(_)
            | Operation::Byte(_)
            | Operation::Shl(_)
            | Operation::Shr(_)
            | Operation::Sar(_)
            | Operation::Keccak256(_)
            // Ternary
            | Operation::AddMod(_)
            | Operation::MulMod(_)
            // Sink
            | Operation::SStore(_)
            | Operation::TStore(_)
            | Operation::Log0(_)
            | Operation::Log1(_)
            | Operation::Log2(_)
            | Operation::Log3(_)
            | Operation::Log4(_)
            | Operation::CallDataCopy(_)
            | Operation::CodeCopy(_)
            | Operation::ExtCodeCopy(_)
            | Operation::ReturnDataCopy(_)
            | Operation::MemoryCopy(_)
            // System calls
            | Operation::Call(_)
            | Operation::CallCode(_)
            | Operation::DelegateCall(_)
            | Operation::StaticCall(_)
            | Operation::Create(_)
            | Operation::Create2(_)
            // Terminators
            | Operation::Return(_)
            | Operation::Stop(_)
            | Operation::Revert(_)
            | Operation::Invalid(_)
            | Operation::SelfDestruct(_) => self.emit_standard_op(op_view, asm, ctx),
        }
    }
}
