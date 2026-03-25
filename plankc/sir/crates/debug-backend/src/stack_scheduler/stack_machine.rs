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

use sir_assembler::{AsmReference, Assembler, MarkId};
use sir_data::{BasicBlockId, EthIRProgram, LargeConstId, LocalId, Operation};

use crate::{
    MarkMap, TranslationPhase, operations::op_kind_to_direct_op,
    static_memory_layout::StaticMemoryLayout,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum StackEntry {
    Local(LocalId),
    SmallConst(u32),
    LargeConst(LargeConstId),
    Label(MarkId),
    Opaque,
}

pub(crate) struct StackMachine {
    stack: Vec<StackEntry>,
    spill_threshold: u8,
    cleanup_threshold: u16,
}

impl StackMachine {
    pub fn new(spill_threshold: u8, cleanup_threshold: u16) -> Self {
        Self { stack: Vec::new(), spill_threshold, cleanup_threshold }
    }

    pub fn remap_block_inputs(&mut self, inputs: &[LocalId]) {
        for (entry, &local) in self.stack.iter_mut().zip(inputs.iter()) {
            *entry = StackEntry::Local(local);
        }
    }

    pub fn dispatch_block(
        &mut self,
        bb_id: BasicBlockId,
        ir: &EthIRProgram,
        asm: &mut Assembler,
        mark_map: &MarkMap,
        memory_layout: &StaticMemoryLayout,
    ) {
        let block = ir.block(bb_id);
        self.remap_block_inputs(block.inputs());
        for op_view in block.operations() {
            self.dispatch_operation(op_view.op(), asm, ir, mark_map);
        }
        self.build_output_stack(block.outputs(), asm, memory_layout);
    }

    /// Arranges the stack so that only the declared outputs remain, in the
    /// correct order per the calling convention (first output on top).
    /// Values not in memory are loaded back onto the stack.
    fn build_output_stack(
        &mut self,
        outputs: &[LocalId],
        asm: &mut Assembler,
        memory_layout: &StaticMemoryLayout,
    ) {
        self.cleanup_stack(outputs, asm);

        // Walk outputs in reverse (last output first) so that each value
        // pushed/swapped to the top ends up in the correct final position.
        for &output in outputs.iter().rev() {
            // Search for the output within SWAP reach of the current top.
            let depth = self
                .stack
                .iter()
                .rev()
                .position(|e| matches!(e, StackEntry::Local(l) if *l == output));
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
                    // TODO: if output was spilled to memory, reload it:
                    //   asm.push_minimal_u32(memory_layout.get_local_addr(output));
                    //   asm.push_op_byte(sir_assembler::op::MLOAD);
                    //   self.stack.push(StackEntry::Local(output));
                    // TODO: if not in memory, use aggressive dig (SWAP+POP) to
                    //   bring it within SWAP reach, then swap to top.
                    todo!("output not reachable within SWAP depth")
                }
            }
        }
    }

    fn cleanup_stack(&mut self, outputs: &[LocalId], asm: &mut Assembler) {
        self.basic_cleanup(outputs, asm);

        if self.stack.len() > self.cleanup_threshold as usize {
            self.aggressive_cleanup(outputs, asm);
        }
    }

    /// Pops from the top of the stack: removes non-outputs and duplicate outputs.
    /// Stops at a unique output or the last output (already in its final position).
    fn basic_cleanup(&mut self, outputs: &[LocalId], asm: &mut Assembler) {
        if outputs.is_empty() {
            return;
        }
        while let Some(top) = self.stack.last() {
            let local = match top {
                StackEntry::Local(local) => *local,
                _ => {
                    self.stack.pop();
                    asm.push_op_byte(sir_assembler::op::POP);
                    continue;
                }
            };

            if !outputs.contains(&local) {
                self.stack.pop();
                asm.push_op_byte(sir_assembler::op::POP);
                continue;
            }

            // Last output is already in its final bottom position — stop.
            if outputs.last() == Some(&local) {
                break;
            }

            // It's an output — only pop if another copy exists within SWAP reach
            let reachable_start =
                self.stack.len().saturating_sub(1 + sir_assembler::op::SWAP_LIMIT as usize);
            let has_duplicate = self.stack[reachable_start..self.stack.len() - 1]
                .iter()
                .any(|e| matches!(e, StackEntry::Local(l) if *l == local));
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
        op: Operation,
        asm: &mut Assembler,
        ir: &EthIRProgram,
        mark_map: &MarkMap,
    ) {
        match op {
            // Nullary: 0 in, 1 out, single opcode
            Operation::Address(data)
            | Operation::Origin(data)
            | Operation::Caller(data)
            | Operation::CallValue(data)
            | Operation::CallDataSize(data)
            | Operation::CodeSize(data)
            | Operation::GasPrice(data)
            | Operation::ReturnDataSize(data)
            | Operation::Gas(data)
            | Operation::Coinbase(data)
            | Operation::Timestamp(data)
            | Operation::Number(data)
            | Operation::Difficulty(data)
            | Operation::GasLimit(data)
            | Operation::ChainId(data)
            | Operation::SelfBalance(data)
            | Operation::BaseFee(data)
            | Operation::BlobBaseFee(data) => {
                let evm_op =
                    op_kind_to_direct_op(op.kind()).expect("nullary ops have direct EVM op");
                asm.push_op_byte(evm_op);
                self.stack.push(StackEntry::Local(data.outs[0]));
            }

            // Unary: 1 in, 1 out, single opcode
            Operation::IsZero(_)
            | Operation::Not(_)
            | Operation::Balance(_)
            | Operation::CallDataLoad(_)
            | Operation::ExtCodeSize(_)
            | Operation::ExtCodeHash(_)
            | Operation::BlockHash(_)
            | Operation::SLoad(_)
            | Operation::TLoad(_)
            | Operation::BlobHash(_) => todo!("unary: requires liveness"),

            // Binary: 2 in, 1 out, single opcode
            Operation::Add(_)
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
            | Operation::Keccak256(_) => todo!("binary: requires liveness"),

            // Ternary: 3 in, 1 out, single opcode
            Operation::AddMod(_) | Operation::MulMod(_) => todo!("ternary: requires liveness"),

            // Sink: N in, 0 out, single opcode
            Operation::SStore(_)
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
            | Operation::MemoryCopy(_) => todo!("sink: requires liveness"),

            // Constants: 0 in, 1 out, push a value
            Operation::SetSmallConst(data) => {
                asm.push_minimal_u32(data.value);
                self.stack.push(StackEntry::SmallConst(data.value));
            }
            Operation::SetLargeConst(data) => {
                asm.push_minimal_u256(ir.large_consts[data.value]);
                self.stack.push(StackEntry::LargeConst(data.value));
            }
            Operation::SetDataOffset(data) => {
                let data_mark = mark_map.get_data_mark(data.segment_id);
                mark_map.push_code_offset(asm, data_mark);
                self.stack.push(StackEntry::Label(data_mark));
            }
            Operation::RuntimeStartOffset(_) => {
                debug_assert!(
                    mark_map.phase() == TranslationPhase::Init,
                    "unexpected runtime_start_offset in run code"
                );
                asm.push_reference(AsmReference::new_direct(mark_map.runtime_start));
                self.stack.push(StackEntry::Label(mark_map.runtime_start));
            }
            Operation::InitEndOffset(_) => {
                debug_assert!(
                    mark_map.phase() == TranslationPhase::Init,
                    "unexpected init_end_offset in run code"
                );
                asm.push_reference(AsmReference::new_direct(mark_map.initcode_end));
                self.stack.push(StackEntry::Label(mark_map.initcode_end));
            }
            Operation::RuntimeLength(_) => {
                asm.push_reference(AsmReference::new_delta(
                    mark_map.runtime_start,
                    mark_map.initcode_end,
                ));
                self.stack.push(StackEntry::Label(mark_map.runtime_start));
            }

            // Memory: allocation and memory I/O
            Operation::DynamicAllocZeroed(_)
            | Operation::DynamicAllocAnyBytes(_)
            | Operation::AcquireFreePointer(_)
            | Operation::StaticAllocZeroed(_)
            | Operation::StaticAllocAnyBytes(_)
            | Operation::MemoryLoad(_)
            | Operation::MemoryStore(_) => todo!("memory: requires liveness"),

            // System calls: N in, M out, single opcode
            Operation::Call(_)
            | Operation::CallCode(_)
            | Operation::DelegateCall(_)
            | Operation::StaticCall(_)
            | Operation::Create(_)
            | Operation::Create2(_) => todo!("system call: requires liveness"),

            // Terminators: N in, 0 out, halt execution
            Operation::Return(_)
            | Operation::Stop(_)
            | Operation::Revert(_)
            | Operation::Invalid(_)
            | Operation::SelfDestruct(_) => todo!("terminator: requires liveness"),

            // Control: internal function call
            Operation::InternalCall(_) => todo!("icall: requires liveness"),

            // Trivial: copy and noop
            Operation::SetCopy(_) => todo!("copy: requires liveness"),
            Operation::Noop(_) => {}
        }
    }
}
