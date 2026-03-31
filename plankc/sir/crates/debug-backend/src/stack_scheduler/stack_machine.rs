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

enum DispatchMode<'a> {
    Simulate { needs_spill: &'a mut DenseIndexSet<LocalId> },
    Emit { asm: &'a mut Assembler, needs_spill: &'a DenseIndexSet<LocalId> },
}

pub(crate) struct StackMachine {
    stack: Vec<StackEntry>,
    in_memory: DenseIndexSet<LocalId>,
    remap_buf: Vec<LocalId>,
    spill_threshold: u8,
    #[cfg(debug_assertions)]
    pub(crate) spill_count: usize,
}

impl StackMachine {
    pub fn new(spill_threshold: u8) -> Self {
        Self {
            stack: Vec::new(),
            in_memory: DenseIndexSet::new(),
            remap_buf: Vec::new(),
            spill_threshold,
            #[cfg(debug_assertions)]
            spill_count: 0,
        }
    }

    /// Runs the simulation pass over operations to identify locals that would be
    /// unreachable (depth >= threshold) at some use site. Saves and restores self.stack.
    fn simulate_for_spills<'a>(
        &mut self,
        operations: impl Iterator<Item = OperationView<'a>>,
        ctx: &mut TranslationContext,
    ) -> DenseIndexSet<LocalId> {
        let saved_stack = self.stack.clone();
        let mut needs_spill = DenseIndexSet::new();
        {
            let mut mode = DispatchMode::Simulate { needs_spill: &mut needs_spill };
            for op_view in operations {
                self.dispatch_operation(op_view, &mut mode, ctx);
            }
        }
        self.stack = saved_stack;
        needs_spill
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
    /// In Simulate mode, flags locals that would be unreachable (depth >= threshold).
    /// Dead copies remain until `arrange_stack_to_layout` cleans them up at block end.
    fn prepare_input(&mut self, local: LocalId, mode: &mut DispatchMode, ctx: &TranslationContext) {
        let depth = self.stack.iter().rev().position(|&e| e == StackEntry::Local(local));
        let threshold = self.spill_threshold as usize;
        match depth {
            Some(d) if d < threshold => {
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_op_byte(op::dup_n(d as u8 + 1));
                }
            }
            _ => match mode {
                DispatchMode::Simulate { needs_spill } => {
                    needs_spill.add(local);
                }
                DispatchMode::Emit { asm, .. } => {
                    assert!(
                        self.in_memory.contains(local),
                        "local {local} is not reachable on stack or in memory"
                    );
                    ctx.memory_layout.emit_local_load(asm, local);
                }
            },
        }
        self.stack.push(StackEntry::Local(local));
    }

    /// Pushes a local onto the symbolic stack. In Emit mode, if the local is in
    /// the spill set, emits DUP1 + MSTORE to back it to memory while keeping it
    /// on the stack.
    fn push_local(
        &mut self,
        local: LocalId,
        mode: &mut DispatchMode,
        memory_layout: &StaticMemoryLayout,
    ) {
        self.stack.push(StackEntry::Local(local));
        if let DispatchMode::Emit { asm, needs_spill } = mode
            && needs_spill.contains(local)
        {
            asm.push_op_byte(op::DUP1);
            memory_layout.emit_local_store(asm, local);
            self.in_memory.add(local);
            #[cfg(debug_assertions)]
            {
                self.spill_count += 1;
            }
        }
    }

    /// Generic handler for operations with a direct EVM opcode.
    /// Prepares all inputs on the stack (in EVM operand order), emits the
    /// opcode, then updates the symbolic stack with the outputs.
    fn emit_standard_op(
        &mut self,
        op_view: OperationView,
        mode: &mut DispatchMode,
        ctx: &TranslationContext,
    ) {
        let inputs = op_view.inputs();
        for input in inputs.iter().rev() {
            self.prepare_input(*input, mode, ctx);
        }

        if let DispatchMode::Emit { asm, .. } = mode {
            let evm_op = op_kind_to_direct_op(op_view.op().kind())
                .expect("standard op has direct EVM mapping");
            asm.push_op_byte(evm_op);
        }

        self.stack.truncate(self.stack.len() - inputs.len());

        for output in op_view.outputs() {
            self.push_local(*output, mode, ctx.memory_layout);
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

        let needs_spill = self.simulate_for_spills(block.operations(), ctx);

        // Spill layout values that the simulation flagged (they're on stack from block entry)
        for i in 0..self.stack.len() {
            if let StackEntry::Local(local) = self.stack[i]
                && needs_spill.contains(local)
            {
                let depth = self.stack.len() - 1 - i;
                assert!(
                    depth < self.spill_threshold as usize,
                    "layout value at depth >= spill_threshold cannot be spilled at block entry"
                );
                asm.push_op_byte(op::dup_n(depth as u8 + 1));
                ctx.memory_layout.emit_local_store(asm, local);
                self.in_memory.add(local);
            }
        }

        // Pass 2: codegen with spill stores
        {
            let mut mode = DispatchMode::Emit { asm: &mut *asm, needs_spill: &needs_spill };
            for op_view in block.operations() {
                self.dispatch_operation(op_view, &mut mode, ctx);
            }
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
        mode: &mut DispatchMode,
        ctx: &mut TranslationContext,
    ) {
        let op = op_view.op();
        match op {
            // Constants: 0 in, 1 out, push a value
            Operation::SetSmallConst(data) => {
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_minimal_u32(data.value);
                }
                self.push_local(data.sets, mode, ctx.memory_layout);
            }
            Operation::SetLargeConst(data) => {
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_minimal_u256(ctx.ir.large_consts[data.value]);
                }
                self.push_local(data.sets, mode, ctx.memory_layout);
            }
            Operation::SetDataOffset(data) => {
                if let DispatchMode::Emit { asm, .. } = mode {
                    let data_mark = ctx.mark_map.get_data_mark(data.segment_id);
                    ctx.mark_map.emit_code_offset_push(asm, data_mark);
                }
                self.push_local(data.sets, mode, ctx.memory_layout);
            }
            Operation::RuntimeStartOffset(data) => {
                debug_assert!(
                    ctx.mark_map.phase() == TranslationPhase::Init,
                    "unexpected runtime_start_offset in run code"
                );
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_reference(AsmReference::new_direct(ctx.mark_map.runtime_start));
                }
                self.push_local(data.outs[0], mode, ctx.memory_layout);
            }
            Operation::InitEndOffset(data) => {
                debug_assert!(
                    ctx.mark_map.phase() == TranslationPhase::Init,
                    "unexpected init_end_offset in run code"
                );
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_reference(AsmReference::new_direct(ctx.mark_map.initcode_end));
                }
                self.push_local(data.outs[0], mode, ctx.memory_layout);
            }
            Operation::RuntimeLength(data) => {
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_reference(AsmReference::new_delta(
                        ctx.mark_map.runtime_start,
                        ctx.mark_map.initcode_end,
                    ));
                }
                self.push_local(data.outs[0], mode, ctx.memory_layout);
            }

            // Memory: allocation and memory I/O
            Operation::AcquireFreePointer(InlineOperands { ins: [], outs: [dst] }) => {
                if let DispatchMode::Emit { asm, .. } = mode {
                    ctx.memory_layout.emit_free_ptr_load(asm);
                }
                self.push_local(dst, mode, ctx.memory_layout);
            }

            Operation::MemoryLoad(data) => {
                self.prepare_input(data.ptr, mode, ctx);
                // evm: [ptr]                                   symbolic: [ptr]
                self.stack.pop();
                if let DispatchMode::Emit { asm, .. } = mode {
                    let load_size = data.size as u32;
                    asm.push_op_byte(op::MLOAD);
                    // evm: [raw_word]                              symbolic: []
                    asm.push_minimal_u32(256 - load_size * 8);
                    asm.push_op_byte(op::SHR);
                }
                self.push_local(data.out, mode, ctx.memory_layout);
                // evm: [value]                                 symbolic: [out]
            }
            Operation::MemoryStore(data) => {
                self.prepare_input(data.ptr(), mode, ctx);
                // evm: [ptr]                                   symbolic: [ptr]
                if let DispatchMode::Emit { asm, .. } = mode {
                    let load_size = data.size as u32;
                    let shift_to_clean_word = load_size * 8;
                    asm.push_op_byte(op::DUP1);
                    // evm: [ptr, ptr]                              symbolic: [ptr]
                    asm.push_op_byte(op::MLOAD);
                    // evm: [current_word, ptr]                     symbolic: [ptr]
                    asm.push_minimal_u32(shift_to_clean_word);
                    asm.push_op_byte(op::SHL);
                    // evm: [current_word << shift, ptr]            symbolic: [ptr]
                    asm.push_minimal_u32(shift_to_clean_word);
                    asm.push_op_byte(op::SHR);
                }
                self.stack.push(StackEntry::Intermediate);
                // evm: [cleaned_word, ptr]                     symbolic: [Intermediate, ptr]
                self.prepare_input(data.value(), mode, ctx);
                // evm: [value, cleaned_word, ptr]              symbolic: [value, Intermediate, ptr]
                if let DispatchMode::Emit { asm, .. } = mode {
                    let load_size = data.size as u32;
                    asm.push_minimal_u32(256 - load_size * 8);
                    asm.push_op_byte(op::SHL);
                    // evm: [shifted_value, cleaned_word, ptr]      symbolic: [value, Intermediate, ptr]
                    asm.push_op_byte(op::OR);
                }
                self.stack.pop(); // value
                self.stack.pop(); // Intermediate
                self.stack.push(StackEntry::Intermediate); // updated_word
                // evm: [updated_word, ptr]                     symbolic: [Intermediate, ptr]
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_op_byte(op::SWAP1);
                    // evm: [ptr, updated_word]                     symbolic: [Intermediate, ptr]
                    asm.push_op_byte(op::MSTORE);
                }
                self.stack.pop(); // Intermediate
                self.stack.pop(); // ptr
                // evm: []                                      symbolic: []
            }

            Operation::DynamicAllocZeroed(InlineOperands { ins: [size], outs: [ptr_out] })
            | Operation::DynamicAllocAnyBytes(InlineOperands { ins: [size], outs: [ptr_out] }) => {
                if let DispatchMode::Emit { asm, .. } = mode {
                    ctx.memory_layout.emit_free_ptr_load(asm);
                }
                self.stack.push(StackEntry::Intermediate);
                // evm: [free_ptr]                                    symbolic: [Intermediate]
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_op_byte(op::DUP1);
                }
                self.stack.push(StackEntry::Intermediate);
                // evm: [free_ptr, free_ptr]                          symbolic: [Intermediate, Intermediate]
                self.prepare_input(size, mode, ctx);
                // evm: [size, free_ptr, free_ptr]                    symbolic: [size, Intermediate, Intermediate]
                if let DispatchMode::Emit { asm, .. } = mode {
                    asm.push_op_byte(op::DUP1);
                    asm.push_op_byte(op::CALLDATASIZE);
                    asm.push_op_byte(op::DUP4);
                    asm.push_op_byte(op::CALLDATACOPY);
                    // evm: [size, free_ptr, free_ptr]                    symbolic: [size, Intermediate, Intermediate]
                    asm.push_op_byte(op::ADD);
                    // evm: [free_ptr', free_ptr]                         symbolic: [size, Intermediate, Intermediate]
                    asm.push_minimal_u32(ctx.memory_layout.free_pointer);
                    asm.push_op_byte(op::MSTORE);
                }
                // evm: [free_ptr]                                    symbolic: [size, Intermediate, Intermediate]
                self.stack.pop(); // size
                self.stack.pop(); // Intermediate
                self.stack.pop(); // Intermediate
                self.push_local(ptr_out, mode, ctx.memory_layout);
                // evm: [free_ptr]                                    symbolic: [ptr_out]
            }
            Operation::StaticAllocZeroed(data)
            | Operation::StaticAllocAnyBytes(data) => {
                if let DispatchMode::Emit { asm, .. } = mode {
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
                }
                self.push_local(data.ptr_out, mode, ctx.memory_layout);
                // evm: [free_ptr]                                    symbolic: [ptr_out]
            }

            Operation::InternalCall(data) => {
                let inputs = data.get_inputs(ctx.ir);
                let outputs = data.get_outputs(ctx.ir);

                for input in inputs.iter().rev() {
                    self.prepare_input(*input, mode, ctx);
                }
                // evm: [arg1, ..., argN]                      symbolic: [arg1, ..., argN]

                if let DispatchMode::Emit { asm, .. } = mode {
                    // Store return address to memory
                    let return_mark = ctx.mark_map.allocate_mark();
                    let return_store_loc = ctx.memory_layout.get_return_dest_store(data.function);
                    ctx.mark_map.emit_code_offset_push(asm, return_mark);
                    asm.push_minimal_u32(return_store_loc);
                    asm.push_op_byte(op::MSTORE);

                    // Jump to callee entry
                    let func_entry_bb = ctx.ir.function(data.function).entry().id();
                    let func_entry_bb_mark = ctx.mark_map.get_bb_mark(func_entry_bb);
                    ctx.mark_map.emit_code_offset_push(asm, func_entry_bb_mark);
                    asm.push_op_byte(op::JUMP);

                    // Return lands here
                    asm.push_mark(return_mark);
                    asm.push_op_byte(op::JUMPDEST);

                    // Enqueue callee for translation
                    ctx.bbs_to_be_translated.push((data.function, func_entry_bb));
                }

                // Update symbolic stack: pop args, push outputs
                for _ in inputs {
                    self.stack.pop();
                }
                for output in outputs {
                    self.push_local(*output, mode, ctx.memory_layout);
                }
                // evm: [out1, ..., outM]                      symbolic: [out1, ..., outM]
            }

            Operation::SetCopy(InlineOperands { ins: [src], outs: [dst] }) => {
                self.prepare_input(src, mode, ctx);
                self.stack.pop();
                self.push_local(dst, mode, ctx.memory_layout);
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
            | Operation::SelfDestruct(_) => self.emit_standard_op(op_view, mode, ctx),
        }
    }
}
