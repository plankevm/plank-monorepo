//! Immutables are compiled into `PUSH<size>` placeholders with zeroed immediates in the runtime
//! code. `setimmutable` patches every placeholder of its immutable in the runtime code copy that
//! initcode is assembling in memory (at `runtime_ptr`), so the copy must already be in place. Each
//! placeholder is overwritten, a repeated `setimmutable` replaces the previously set value. An
//! immutable without placeholders just has its operands popped.
//!
//! ## Write strategies
//!
//! Stack on entry is `[value, runtime_ptr]` (top last), `N` is the number of placeholders and the
//! gas numbers exclude memory expansion:
//!
//! | size     | strategy               | gas       |
//! |----------|------------------------|-----------|
//! | 32       | `mstore` each          | 15N - 6   |
//! | 1        | `mstore8` each         | 15N - 6   |
//! | 2..=31   | scratch + `mcopy` each | 21N + 9   |
//!
//! The scratch slot is reserved whenever the runtime references any immutable of a partial size.
//!
//! For partial sizes the value is `mstore`d to the scratch slot once (12 gas) and its low `size`
//! bytes are then `mcopy`d into every placeholder (21 gas each, 18 for the last). Only the
//! placeholder bytes are written, so adjacent runtime code is left untouched and no memory past
//! the runtime copy is accessed. A `mcopy` costs 21 gas vs. 15 for `mstore`/`mstore8`, which is
//! why it's not used for full words or single bytes. In all cases only the low `size` bytes of
//! `value` end up in the placeholder.

use plank_core::{Idx, IndexVec, Span};
use sir_assembler::{AsmReference, Assembler, MarkId, MarkReference, op};
use sir_data::{EthIRProgram, ImmutableId, operation::ByteSize};
use sir_static_memory_allocator::EvmMemAddr;
use smallvec::SmallVec;

const PLACEHOLDERS_INLINE_CAPACITY: usize = 8;
const EVM_WORD_IN_BYTES: u32 = 0x20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteStrategy {
    StoreEach { store_op: u8 },
    CopyFromScratch,
}

impl WriteStrategy {
    pub fn select(size: ByteSize) -> Self {
        match size {
            ByteSize::B32 => Self::StoreEach { store_op: op::MSTORE },
            ByteSize::B1 => Self::StoreEach { store_op: op::MSTORE8 },
            _ => Self::CopyFromScratch,
        }
    }
}

/// Placeholder marks of the `getimmutable` operations reachable from the runtime entry point,
/// contiguous per immutable. `ends` holds each immutable's range end relative to `marks_start`;
/// emitting a placeholder takes it by decrementing the end, so `placeholders` is only valid before
/// runcode is emitted.
#[derive(Debug)]
pub(crate) struct ImmutableRefs {
    marks_start: MarkId,
    ends: IndexVec<ImmutableId, u32>,
}

impl ImmutableRefs {
    pub fn new(ir: &EthIRProgram) -> Self {
        Self { marks_start: MarkId::ZERO, ends: IndexVec::from_vec(vec![0; ir.immutables.len()]) }
    }

    pub fn count_ref(&mut self, immutable: ImmutableId) {
        self.ends[immutable] += 1;
    }

    pub fn alloc_marks(&mut self, next_mark_id: &mut MarkId) {
        let mut end = 0;
        for count in self.ends.iter_mut() {
            end += *count;
            *count = end;
        }
        self.marks_start = *next_mark_id;
        *next_mark_id += end;
    }

    fn placeholders(&self, immutable: ImmutableId) -> Span<MarkId> {
        let start = self.ends[ImmutableId::ZERO..immutable].last().copied().unwrap_or(0);
        Span::new(self.marks_start + start, self.marks_start + self.ends[immutable])
    }

    pub fn emit_placeholder(
        &mut self,
        asm: &mut Assembler,
        size: ByteSize,
        immutable: ImmutableId,
    ) {
        self.ends[immutable] -= 1;
        asm.push_placeholder_push(size, self.marks_start + self.ends[immutable]);
    }

    pub fn needs_scratch(&self, ir: &EthIRProgram) -> bool {
        ir.immutables.enumerate_idx().any(|(immutable, &size)| {
            WriteStrategy::select(size) == WriteStrategy::CopyFromScratch
                && !self.placeholders(immutable).is_empty()
        })
    }

    pub fn emit_set(
        &self,
        asm: &mut Assembler,
        runcode_start: MarkId,
        scratch_slot: Option<EvmMemAddr>,
        immutable: ImmutableId,
        size: ByteSize,
    ) {
        let placeholders: SmallVec<[MarkId; PLACEHOLDERS_INLINE_CAPACITY]> =
            self.placeholders(immutable).iter().collect();
        let push_offset = |asm: &mut Assembler, placeholder: MarkId| {
            let offset = MarkReference::Delta(Span::new(runcode_start, placeholder));
            asm.push_reference(AsmReference::pushed(offset));
        };

        let Some((&last, rest)) = placeholders.split_last() else {
            asm.push_op_byte(op::POP);
            asm.push_op_byte(op::POP);
            return;
        };

        // Stack comments show deepest => highest.
        match WriteStrategy::select(size) {
            WriteStrategy::StoreEach { store_op } => {
                for &placeholder in rest {
                    // input: [value, ptr]
                    asm.push_op_byte(op::DUP2); //         [value, ptr, value]
                    asm.push_op_byte(op::DUP2); //         [value, ptr, value, ptr]
                    push_offset(asm, placeholder); //      [value, ptr, value, ptr, offset]
                    asm.push_op_byte(op::ADD); //          [value, ptr, value, dst]
                    asm.push_op_byte(store_op); //         [value, ptr]
                }
                push_offset(asm, last); //                 [value, ptr, offset]
                asm.push_op_byte(op::ADD); //              [value, dst]
                asm.push_op_byte(store_op); //             []
            }
            WriteStrategy::CopyFromScratch => {
                let scratch = scratch_slot.expect("missing setimmutable scratch slot").get();
                let copy_size = size as u32;
                let src = scratch + EVM_WORD_IN_BYTES - copy_size;

                // input: [value, ptr]
                asm.push_minimal_u32(copy_size); //        [value, ptr, size]
                asm.push_op_byte(op::SWAP2); //            [size, ptr, value]
                asm.push_minimal_u32(scratch); //          [size, ptr, value, scratch]
                asm.push_op_byte(op::MSTORE); //           [size, ptr]
                for &placeholder in rest {
                    asm.push_op_byte(op::DUP2); //         [size, ptr, size]
                    asm.push_minimal_u32(src); //          [size, ptr, size, src]
                    asm.push_op_byte(op::DUP3); //         [size, ptr, size, src, ptr]
                    push_offset(asm, placeholder); //      [size, ptr, size, src, ptr, offset]
                    asm.push_op_byte(op::ADD); //          [size, ptr, size, src, dst]
                    asm.push_op_byte(op::MCOPY); //        [size, ptr]
                }
                asm.push_minimal_u32(src); //              [size, ptr, src]
                asm.push_op_byte(op::SWAP1); //            [size, src, ptr]
                push_offset(asm, last); //                 [size, src, ptr, offset]
                asm.push_op_byte(op::ADD); //              [size, src, dst]
                asm.push_op_byte(op::MCOPY); //            []
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use plank_core::IncIterable;

    fn static_gas(code: &[u8]) -> u32 {
        let mut gas = 0;
        let mut pc = 0;
        while pc < code.len() {
            let opcode = code[pc];
            gas += match opcode {
                op::PUSH0 => 2,
                op::MCOPY => 3 + 3,
                op::POP
                | op::ADD
                | op::OR
                | op::SHL
                | op::MLOAD
                | op::MSTORE
                | op::MSTORE8
                | op::DUP1..=op::DUP16
                | op::SWAP1..=op::SWAP16
                | op::PUSH1..=op::PUSH32 => 3,
                _ => panic!("unexpected opcode {}", op::name(opcode)),
            };
            pc += 1 + op::push_size(opcode).map_or(0, |size| size as usize);
        }
        gas
    }

    const TEST_SCRATCH_SLOT: EvmMemAddr = EvmMemAddr::new(0x40);

    struct Emitted {
        asm: String,
        set_gas: u32,
        set_bytes: usize,
    }

    fn emit_set_with_refs(size: ByteSize, ref_count: u32) -> Emitted {
        let mut next_mark = MarkId::ZERO;
        let runcode_start = next_mark.get_and_inc();
        let immutable = ImmutableId::new(0);
        let mut refs = ImmutableRefs {
            marks_start: MarkId::ZERO,
            ends: IndexVec::from_vec(vec![ref_count, 1]),
        };
        refs.alloc_marks(&mut next_mark);

        let mut asm = Assembler::with_capacity(256, 32);
        refs.emit_set(&mut asm, runcode_start, Some(TEST_SCRATCH_SLOT), immutable, size);
        asm.push_mark(runcode_start);
        for i in 0..=ref_count {
            let emitted = if i < ref_count { immutable } else { ImmutableId::new(1) };
            refs.emit_placeholder(&mut asm, size, emitted);
            asm.push_op_byte(op::POP);
        }

        let mut code = Vec::new();
        let mark_offsets = asm.assemble(&mut code, Some(next_mark.idx())).unwrap();
        let set_code = &code[..mark_offsets[runcode_start] as usize];
        Emitted { asm: asm.to_string(), set_gas: static_gas(set_code), set_bytes: set_code.len() }
    }

    #[test]
    fn strategy_selection() {
        use ByteSize as S;
        assert_eq!(
            WriteStrategy::select(S::B32),
            WriteStrategy::StoreEach { store_op: op::MSTORE }
        );
        assert_eq!(
            WriteStrategy::select(S::B1),
            WriteStrategy::StoreEach { store_op: op::MSTORE8 }
        );
        assert_eq!(WriteStrategy::select(S::B2), WriteStrategy::CopyFromScratch);
        assert_eq!(WriteStrategy::select(S::B31), WriteStrategy::CopyFromScratch);
    }

    #[test]
    fn strategy_gas_matches_documented_costs() {
        use ByteSize as S;
        for n in 1..=5 {
            assert_eq!(emit_set_with_refs(S::B32, n).set_gas, 15 * n - 6, "mstore, n={n}");
            assert_eq!(emit_set_with_refs(S::B1, n).set_gas, 15 * n - 6, "mstore8, n={n}");
            assert_eq!(emit_set_with_refs(S::B7, n).set_gas, 21 * n + 9, "scratch copy, n={n}");
            assert_eq!(emit_set_with_refs(S::B31, n).set_gas, 21 * n + 9, "scratch copy, n={n}");
        }
        assert_eq!(emit_set_with_refs(S::B7, 0).set_gas, 6);
    }

    #[test]
    fn strategy_sizes() {
        use ByteSize as S;
        // Offsets are < 256 here, so each offset push is 2 bytes.
        for n in 1..=5 {
            let n_usize = n as usize;
            assert_eq!(emit_set_with_refs(S::B32, n).set_bytes, 6 * n_usize - 2, "mstore, n={n}");
            assert_eq!(
                emit_set_with_refs(S::B7, n).set_bytes,
                8 * n_usize + 5,
                "scratch copy, n={n}"
            );
        }
    }

    fn assert_set_asm(size: ByteSize, ref_count: u32, expected: &str) {
        pretty_assertions::assert_str_eq!(
            plank_test_utils::dedent_preserve_indent(&emit_set_with_refs(size, ref_count).asm),
            plank_test_utils::dedent_preserve_indent(expected)
        );
    }

    #[test]
    fn pop_unreferenced() {
        assert_set_asm(
            ByteSize::B7,
            0,
            r#"
              POP
              POP
            .mark0:
              PUSH7 0x (truncated)
            .mark1:
              data 0x00000000000000 (7 bytes)
              POP
            "#,
        );
    }

    #[test]
    fn mstore_each_full_word() {
        assert_set_asm(
            ByteSize::B32,
            2,
            r#"
              DUP2
              DUP2
              PUSH (.mark1 - .mark0)
              ADD
              MSTORE
              PUSH (.mark2 - .mark0)
              ADD
              MSTORE
            .mark0:
              PUSH32 0x (truncated)
            .mark2:
              data 0x0000000000000000000000000000000000000000000000000000000000000000 (32 bytes)
              POP
              PUSH32 0x (truncated)
            .mark1:
              data 0x0000000000000000000000000000000000000000000000000000000000000000 (32 bytes)
              POP
              PUSH32 0x (truncated)
            .mark3:
              data 0x0000000000000000000000000000000000000000000000000000000000000000 (32 bytes)
              POP
            "#,
        );
    }

    #[test]
    fn mstore8_each_single_byte() {
        assert_set_asm(
            ByteSize::B1,
            2,
            r#"
              DUP2
              DUP2
              PUSH (.mark1 - .mark0)
              ADD
              MSTORE8
              PUSH (.mark2 - .mark0)
              ADD
              MSTORE8
            .mark0:
              PUSH1 0x (truncated)
            .mark2:
              data 0x00 (1 bytes)
              POP
              PUSH1 0x (truncated)
            .mark1:
              data 0x00 (1 bytes)
              POP
              PUSH1 0x (truncated)
            .mark3:
              data 0x00 (1 bytes)
              POP
            "#,
        );
    }

    #[test]
    fn scratch_copy_single_reference() {
        assert_set_asm(
            ByteSize::B20,
            1,
            r#"
              PUSH1 0x14
              SWAP2
              PUSH1 0x40
              MSTORE
              PUSH1 0x4c
              SWAP1
              PUSH (.mark1 - .mark0)
              ADD
              MCOPY
            .mark0:
              PUSH20 0x (truncated)
            .mark1:
              data 0x0000000000000000000000000000000000000000 (20 bytes)
              POP
              PUSH20 0x (truncated)
            .mark2:
              data 0x0000000000000000000000000000000000000000 (20 bytes)
              POP
            "#,
        );
    }

    #[test]
    fn scratch_copy_each_reference() {
        assert_set_asm(
            ByteSize::B7,
            3,
            r#"
              PUSH1 0x07
              SWAP2
              PUSH1 0x40
              MSTORE
              DUP2
              PUSH1 0x59
              DUP3
              PUSH (.mark1 - .mark0)
              ADD
              MCOPY
              DUP2
              PUSH1 0x59
              DUP3
              PUSH (.mark2 - .mark0)
              ADD
              MCOPY
              PUSH1 0x59
              SWAP1
              PUSH (.mark3 - .mark0)
              ADD
              MCOPY
            .mark0:
              PUSH7 0x (truncated)
            .mark3:
              data 0x00000000000000 (7 bytes)
              POP
              PUSH7 0x (truncated)
            .mark2:
              data 0x00000000000000 (7 bytes)
              POP
              PUSH7 0x (truncated)
            .mark1:
              data 0x00000000000000 (7 bytes)
              POP
              PUSH7 0x (truncated)
            .mark4:
              data 0x00000000000000 (7 bytes)
              POP
            "#,
        );
    }
}
