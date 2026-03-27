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
use sir_data::{BasicBlockId, ControlView, FunctionId, LocalId, Operation, OperationView};
use sir_passes::IntervalEnd;

use crate::{
    MarkMap, TranslationContext, TranslationPhase, operations::op_kind_to_direct_op,
    static_memory_layout::StaticMemoryLayout,
};

pub(crate) struct StackMachine {
    stack: Vec<LocalId>,
    in_memory: DenseIndexSet<LocalId>,
    spill_threshold: u8,
    cleanup_threshold: u16,
}

impl StackMachine {
    pub fn new(spill_threshold: u8, cleanup_threshold: u16) -> Self {
        Self {
            stack: Vec::new(),
            in_memory: DenseIndexSet::new(),
            spill_threshold,
            cleanup_threshold,
        }
    }

    pub fn remap_block_inputs(&mut self, inputs: &[LocalId]) {
        for (entry, &local) in self.stack.iter_mut().zip(inputs.iter()) {
            *entry = local;
        }
    }

    /// Gets `local` to the top of the stack.
    /// - Last use + on stack: SWAP to top (or noop if already there).
    /// - Not last use + on stack: DUP to top.
    /// - Not on stack: load from memory (must have been spilled).
    fn prepare_input(
        &mut self,
        local: LocalId,
        bb_id: BasicBlockId,
        op_view: OperationView,
        asm: &mut Assembler,
        ctx: &TranslationContext,
    ) {
        let is_last_use = ctx.local_liveness.last_use_in_block(local, bb_id)
            == Some(IntervalEnd::At(op_view.id()));
        let depth = self.stack.iter().rev().position(|&e| e == local);
        match depth {
            Some(d @ 0..=15) if is_last_use => {
                if d > 0 {
                    asm.push_op_byte(op::swap_n(d as u8));
                    let top = self.stack.len() - 1;
                    self.stack.swap(top, top - d);
                }
            }
            Some(d) if d < 16 => {
                asm.push_op_byte(op::dup_n(d as u8 + 1));
                self.stack.push(local);
            }
            _ => {
                assert!(
                    self.in_memory.contains(local),
                    "local {local} is not reachable on stack or in memory"
                );
                ctx.memory_layout.emit_local_load(asm, local);
                self.stack.push(local);
            }
        }
    }

    /// Generic handler for operations with a direct EVM opcode.
    /// Prepares all inputs on the stack (in EVM operand order), emits the
    /// opcode, then updates the symbolic stack with the outputs.
    fn emit_standard_op(
        &mut self,
        op_view: OperationView,
        bb_id: BasicBlockId,
        asm: &mut Assembler,
        ctx: &TranslationContext,
    ) {
        let inputs = op_view.inputs();
        for input in inputs.iter().rev() {
            self.prepare_input(*input, bb_id, op_view, asm, ctx);
        }

        let evm_op =
            op_kind_to_direct_op(op_view.op().kind()).expect("standard op has direct EVM mapping");
        asm.push_op_byte(evm_op);

        self.stack.truncate(self.stack.len() - inputs.len());

        for output in op_view.outputs() {
            self.stack.push(*output);
        }
    }

    pub fn dispatch_block(
        &mut self,
        func: FunctionId,
        bb_id: BasicBlockId,
        asm: &mut Assembler,
        ctx: &TranslationContext,
    ) {
        let block = ctx.ir.block(bb_id);
        self.remap_block_inputs(block.inputs());

        let outputs = block.outputs();

        let condition = match block.control() {
            ControlView::Branches { condition, .. } => Some(condition),
            ControlView::Switch(switch) => Some(switch.condition()),
            _ => None,
        };

        for op_view in block.operations() {
            self.dispatch_operation(op_view, bb_id, asm, ctx);
        }

        self.build_output_stack(outputs, condition, asm, ctx.memory_layout);
        if let Some(cond) = condition {
            self.bring_condition_to_top(cond, asm, ctx.memory_layout);
        }
        self.emit_control_flow(block.control(), func, asm, ctx.mark_map, ctx.memory_layout);
    }

    /// Brings the control flow condition to the top of the stack.
    /// Searches the stack first; if not found, loads from memory.
    /// TODO: if the condition is not on the stack, we assume liveness-driven
    /// eviction has already spilled it to memory.
    fn bring_condition_to_top(
        &mut self,
        local: LocalId,
        asm: &mut Assembler,
        memory_layout: &StaticMemoryLayout,
    ) {
        let depth = self.stack.iter().rev().position(|&e| e == local);

        match depth {
            Some(0) => {}
            Some(d @ 1..=16) => {
                asm.push_op_byte(sir_assembler::op::swap_n(d as u8));
                let top = self.stack.len() - 1;
                self.stack.swap(top, top - d);
            }
            _ => {
                // Not reachable on stack — load from memory.
                memory_layout.emit_local_load(asm, local);
                self.stack.push(local);
            }
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
                self.stack.pop();
                mark_map.emit_code_offset_push(asm, mark_map.get_bb_mark(non_zero_target));
                asm.push_op_byte(op::JUMPI);
                mark_map.emit_code_offset_push(asm, mark_map.get_bb_mark(zero_target));
                asm.push_op_byte(op::JUMP);
            }
            ControlView::Switch(switch) => {
                self.stack.pop();
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

    /// Arranges the stack so that only the declared outputs (and optionally the
    /// control flow condition) remain, in the correct order per the calling
    /// convention (first output on top).
    fn build_output_stack(
        &mut self,
        outputs: &[LocalId],
        condition: Option<LocalId>,
        asm: &mut Assembler,
        _memory_layout: &StaticMemoryLayout,
    ) {
        self.cleanup_stack(outputs, condition, asm);

        // Walk outputs in reverse (last output first) so that each value
        // pushed/swapped to the top ends up in the correct final position.
        for &output in outputs.iter().rev() {
            // Search for the output within SWAP reach of the current top.
            let depth = self.stack.iter().rev().position(|&e| e == output);
            match depth {
                Some(0) => {
                    // Already on top — nothing to do.
                }
                Some(d @ 1..=16) => {
                    asm.push_op_byte(sir_assembler::op::swap_n(d as u8));
                    let top = self.stack.len() - 1;
                    self.stack.swap(top, top - d);
                }
                _ => {
                    // TODO: if output was spilled to memory, reload it
                    // TODO: if not in memory, use aggressive dig (SWAP+POP) to
                    //   bring it within SWAP reach, then swap to top
                    todo!("output not reachable within SWAP depth")
                }
            }
        }
    }

    fn cleanup_stack(
        &mut self,
        outputs: &[LocalId],
        condition: Option<LocalId>,
        asm: &mut Assembler,
    ) {
        self.basic_cleanup(outputs, condition, asm);

        if self.stack.len() > self.cleanup_threshold as usize {
            self.aggressive_cleanup(outputs, asm);
        }
    }

    /// Pops from the top of the stack: removes non-outputs and duplicate outputs.
    /// Stops at a unique output or the last output (already in its final position).
    /// Preserves the control flow condition if present.
    fn basic_cleanup(
        &mut self,
        outputs: &[LocalId],
        condition: Option<LocalId>,
        asm: &mut Assembler,
    ) {
        if outputs.is_empty() && condition.is_none() {
            return;
        }
        while let Some(&top) = self.stack.last() {
            // Preserve the condition — treat it like an output for cleanup purposes.
            if condition == Some(top) {
                break;
            }

            if !outputs.contains(&top) {
                self.stack.pop();
                asm.push_op_byte(sir_assembler::op::POP);
                continue;
            }

            // Last output is already in its final bottom position — stop.
            if outputs.last() == Some(&top) {
                break;
            }

            // It's an output — only pop if another copy exists within SWAP reach
            let reachable_start =
                self.stack.len().saturating_sub(1 + sir_assembler::op::SWAP_LIMIT as usize);
            let has_duplicate = self.stack[reachable_start..self.stack.len() - 1].contains(&top);
            if !has_duplicate {
                break;
            }

            self.stack.pop();
            asm.push_op_byte(sir_assembler::op::POP);
        }
    }

    /// Uses SWAP+POP to remove non-outputs buried deeper in the stack.
    fn aggressive_cleanup(&mut self, _outputs: &[LocalId], _asm: &mut Assembler) {
        todo!("aggressive cleanup: SWAP+POP to remove buried non-outputs")
    }

    fn dispatch_operation(
        &mut self,
        op_view: OperationView,
        bb_id: BasicBlockId,
        asm: &mut Assembler,
        ctx: &TranslationContext,
    ) {
        let op = op_view.op();
        match op {
            // Constants: 0 in, 1 out, push a value
            Operation::SetSmallConst(data) => {
                asm.push_minimal_u32(data.value);
                self.stack.push(data.sets);
            }
            Operation::SetLargeConst(data) => {
                asm.push_minimal_u256(ctx.ir.large_consts[data.value]);
                self.stack.push(data.sets);
            }
            Operation::SetDataOffset(data) => {
                let data_mark = ctx.mark_map.get_data_mark(data.segment_id);
                ctx.mark_map.emit_code_offset_push(asm, data_mark);
                self.stack.push(data.sets);
            }
            Operation::RuntimeStartOffset(data) => {
                debug_assert!(
                    ctx.mark_map.phase() == TranslationPhase::Init,
                    "unexpected runtime_start_offset in run code"
                );
                asm.push_reference(AsmReference::new_direct(ctx.mark_map.runtime_start));
                self.stack.push(data.outs[0]);
            }
            Operation::InitEndOffset(data) => {
                debug_assert!(
                    ctx.mark_map.phase() == TranslationPhase::Init,
                    "unexpected init_end_offset in run code"
                );
                asm.push_reference(AsmReference::new_direct(ctx.mark_map.initcode_end));
                self.stack.push(data.outs[0]);
            }
            Operation::RuntimeLength(data) => {
                asm.push_reference(AsmReference::new_delta(
                    ctx.mark_map.runtime_start,
                    ctx.mark_map.initcode_end,
                ));
                self.stack.push(data.outs[0]);
            }

            // Memory: allocation and memory I/O
            Operation::DynamicAllocZeroed(_)
            | Operation::DynamicAllocAnyBytes(_)
            | Operation::AcquireFreePointer(_)
            | Operation::StaticAllocZeroed(_)
            | Operation::StaticAllocAnyBytes(_)
            | Operation::MemoryLoad(_)
            | Operation::MemoryStore(_) => todo!("memory ops"),

            // Internal function call
            Operation::InternalCall(_) => todo!("icall"),

            // Copy
            Operation::SetCopy(_) => todo!("copy"),

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
            | Operation::SelfDestruct(_) => self.emit_standard_op(op_view, bb_id, asm, ctx),
        }
    }
}
