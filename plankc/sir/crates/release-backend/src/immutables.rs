//! Immutables are compiled into `PUSH<size>` placeholders with zeroed immediates in the runtime
//! code. `setimmutable` patches every placeholder of its immutable in the runtime code copy that
//! initcode is assembling in memory (at `runtime_ptr`), so the copy must already be in place and
//! each placeholder must still be zero.
//!
//! ## Write strategies
//!
//! Stack on entry is `[value, runtime_ptr]` (top last), `N` is the number of placeholders and the
//! gas numbers exclude memory expansion:
//!
//! | size     | strategy           | gas       |
//! |----------|--------------------|-----------|
//! | any, N=0 | discard            | 4         |
//! | 32       | `mstore` each      | 15N - 6   |
//! | 1        | `mstore8` each     | 15N - 6   |
//! | 2..=31   | OR-merge (N=1)     | 38        |
//! | 2..=31   | OR-merge + `mcopy` | 21N + 12  |
//!
//! For partial sizes the value is shifted to be left-aligned once and OR-ed into the word that
//! starts at the first placeholder (27 gas), all other placeholders are then filled with a
//! `size`-byte `mcopy` from the first one (21 gas each, 18 for the last). OR-merging every
//! placeholder instead costs `27N + 11` gas and more bytes. A `mcopy` costs 21 gas vs. 15 for
//! `mstore`/`mstore8`, which is why it's not used for full words or single bytes. The OR-merge's
//! word may extend up to 31 bytes past the runtime copy; those bytes are written back unchanged.

use plank_core::Span;
use sir_assembler::{AsmReference, Assembler, MarkId, MarkReference, op};
use sir_data::{ImmutableId, OperationIdx, operation::IRMemoryIOByteSize};
use smallvec::SmallVec;

const PLACEHOLDERS_INLINE_CAPACITY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteStrategy {
    Discard,
    StoreEach { store_op: u8 },
    OrMerge,
    OrMergeThenCopy,
}

impl WriteStrategy {
    pub fn select(size: IRMemoryIOByteSize, placeholder_count: usize) -> Self {
        match (size, placeholder_count) {
            (_, 0) => Self::Discard,
            (IRMemoryIOByteSize::B32, _) => Self::StoreEach { store_op: op::MSTORE },
            (IRMemoryIOByteSize::B1, _) => Self::StoreEach { store_op: op::MSTORE8 },
            (_, 1) => Self::OrMerge,
            (_, _) => Self::OrMergeThenCopy,
        }
    }
}

#[derive(Debug)]
struct ImmutableRef {
    immutable: ImmutableId,
    get_op: OperationIdx,
    placeholder: MarkId,
    emitted: bool,
}

/// The `getimmutable` operations reachable from the runtime entry point, each with the mark of its
/// placeholder's immediate.
#[derive(Debug, Default)]
pub(crate) struct ImmutableRefs {
    refs: Vec<ImmutableRef>,
}

impl ImmutableRefs {
    pub fn push(&mut self, immutable: ImmutableId, get_op: OperationIdx, placeholder: MarkId) {
        self.refs.push(ImmutableRef { immutable, get_op, placeholder, emitted: false });
    }

    pub fn emit_placeholder(
        &mut self,
        asm: &mut Assembler,
        size: IRMemoryIOByteSize,
        op: OperationIdx,
    ) {
        let imm_ref = self
            .refs
            .iter_mut()
            .find(|imm_ref| imm_ref.get_op == op)
            .expect("getimmutable not collected as runtime reference");
        assert!(!imm_ref.emitted, "getimmutable placeholder emitted twice");
        imm_ref.emitted = true;
        asm.push_placeholder_push(size as u8, imm_ref.placeholder);
    }

    /// Every collected placeholder mark must be placed, otherwise `setimmutable` would patch
    /// whatever code ends up at the unplaced mark's default offset.
    pub fn assert_all_emitted(&self) {
        assert!(
            self.refs.iter().all(|imm_ref| imm_ref.emitted),
            "immutable placeholder collected but never emitted"
        );
    }

    pub fn emit_set(
        &self,
        asm: &mut Assembler,
        runcode_start: MarkId,
        immutable: ImmutableId,
        size: IRMemoryIOByteSize,
    ) {
        let placeholders: SmallVec<[MarkId; PLACEHOLDERS_INLINE_CAPACITY]> = self
            .refs
            .iter()
            .filter(|imm_ref| imm_ref.immutable == immutable)
            .map(|imm_ref| imm_ref.placeholder)
            .collect();
        let push_offset = |asm: &mut Assembler, placeholder: MarkId| {
            let offset = MarkReference::Delta(Span::new(runcode_start, placeholder));
            asm.push_reference(AsmReference::pushed(offset));
        };

        // Stack comments show deepest => highest.
        match WriteStrategy::select(size, placeholders.len()) {
            WriteStrategy::Discard => {
                asm.push_op_byte(op::POP);
                asm.push_op_byte(op::POP);
            }
            WriteStrategy::StoreEach { store_op } => {
                let (&last, rest) = placeholders.split_last().expect("no placeholders");
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
            WriteStrategy::OrMerge => {
                Self::emit_left_align(asm, size); //       [ptr, aligned]
                asm.push_op_byte(op::SWAP1); //            [aligned, ptr]
                push_offset(asm, placeholders[0]); //      [aligned, ptr, offset]
                asm.push_op_byte(op::ADD); //              [aligned, dst]
                asm.push_op_byte(op::DUP1); //             [aligned, dst, dst]
                asm.push_op_byte(op::MLOAD); //            [aligned, dst, word]
                asm.push_op_byte(op::DUP3); //             [aligned, dst, word, aligned]
                asm.push_op_byte(op::OR); //               [aligned, dst, patched_word]
                asm.push_op_byte(op::SWAP1); //            [aligned, patched_word, dst]
                asm.push_op_byte(op::MSTORE); //           [aligned]
                asm.push_op_byte(op::POP); //              []
            }
            WriteStrategy::OrMergeThenCopy => {
                let (&first, rest) = placeholders.split_first().expect("no placeholders");
                let (&last, middle) = rest.split_last().expect("less than 2 placeholders");
                let copy_size = size as u32;

                Self::emit_left_align(asm, size); //       [ptr, aligned]
                asm.push_op_byte(op::DUP2); //             [ptr, aligned, ptr]
                push_offset(asm, first); //                [ptr, aligned, ptr, offset]
                asm.push_op_byte(op::ADD); //              [ptr, aligned, src]
                asm.push_op_byte(op::SWAP1); //            [ptr, src, aligned]
                asm.push_op_byte(op::DUP2); //             [ptr, src, aligned, src]
                asm.push_op_byte(op::MLOAD); //            [ptr, src, aligned, word]
                asm.push_op_byte(op::OR); //               [ptr, src, patched_word]
                asm.push_op_byte(op::DUP2); //             [ptr, src, patched_word, src]
                asm.push_op_byte(op::MSTORE); //           [ptr, src]
                for &placeholder in middle {
                    asm.push_minimal_u32(copy_size); //         [ptr, src, size]
                    asm.push_op_byte(op::DUP2); //         [ptr, src, size, src]
                    asm.push_op_byte(op::DUP4); //         [ptr, src, size, src, ptr]
                    push_offset(asm, placeholder); //      [ptr, src, size, src, ptr, offset]
                    asm.push_op_byte(op::ADD); //          [ptr, src, size, src, dst]
                    asm.push_op_byte(op::MCOPY); //        [ptr, src]
                }
                asm.push_minimal_u32(copy_size); //        [ptr, src, size]
                asm.push_op_byte(op::SWAP2); //            [size, src, ptr]
                push_offset(asm, last); //                 [size, src, ptr, offset]
                asm.push_op_byte(op::ADD); //              [size, src, dst]
                asm.push_op_byte(op::MCOPY); //            []
            }
        }
    }

    fn emit_left_align(asm: &mut Assembler, size: IRMemoryIOByteSize) {
        // input: [value, ptr]
        asm.push_op_byte(op::SWAP1); //                    [ptr, value]
        asm.push_minimal_u32(256 - u32::from(size.bits())); // [ptr, value, shift]
        asm.push_op_byte(op::SHL); //                      [ptr, aligned]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_core::{Idx, IncIterable};

    fn static_gas(code: &[u8]) -> u32 {
        let mut gas = 0;
        let mut pc = 0;
        while pc < code.len() {
            let opcode = code[pc];
            gas += match opcode {
                op::PUSH0 | op::POP => 2,
                op::MCOPY => 3 + 3,
                op::ADD
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
            pc += 1 + op::push_size(opcode).unwrap_or(0) as usize;
        }
        gas
    }

    struct Emitted {
        asm: String,
        set_gas: u32,
        set_bytes: usize,
    }

    fn emit_set_with_refs(size: IRMemoryIOByteSize, ref_count: u32) -> Emitted {
        let mut next_mark = MarkId::ZERO;
        let runcode_start = next_mark.get_and_inc();
        let immutable = ImmutableId::new(0);
        let mut refs = ImmutableRefs::default();
        for i in 0..ref_count {
            refs.push(immutable, OperationIdx::new(i), next_mark.get_and_inc());
        }
        refs.push(ImmutableId::new(1), OperationIdx::new(ref_count), next_mark.get_and_inc());

        let mut asm = Assembler::with_capacity(256, 32);
        refs.emit_set(&mut asm, runcode_start, immutable, size);
        asm.push_mark(runcode_start);
        for i in 0..=ref_count {
            refs.emit_placeholder(&mut asm, size, OperationIdx::new(i));
            asm.push_op_byte(op::POP);
        }
        refs.assert_all_emitted();

        let mut code = Vec::new();
        let mark_offsets = asm.assemble(&mut code, Some(next_mark.idx())).unwrap();
        let set_code = &code[..mark_offsets[runcode_start] as usize];
        Emitted { asm: asm.to_string(), set_gas: static_gas(set_code), set_bytes: set_code.len() }
    }

    #[test]
    fn strategy_selection() {
        use IRMemoryIOByteSize as S;
        assert_eq!(WriteStrategy::select(S::B32, 0), WriteStrategy::Discard);
        assert_eq!(WriteStrategy::select(S::B7, 0), WriteStrategy::Discard);
        assert_eq!(
            WriteStrategy::select(S::B32, 3),
            WriteStrategy::StoreEach { store_op: op::MSTORE }
        );
        assert_eq!(
            WriteStrategy::select(S::B1, 3),
            WriteStrategy::StoreEach { store_op: op::MSTORE8 }
        );
        assert_eq!(WriteStrategy::select(S::B2, 1), WriteStrategy::OrMerge);
        assert_eq!(WriteStrategy::select(S::B31, 1), WriteStrategy::OrMerge);
        assert_eq!(WriteStrategy::select(S::B2, 2), WriteStrategy::OrMergeThenCopy);
        assert_eq!(WriteStrategy::select(S::B31, 5), WriteStrategy::OrMergeThenCopy);
    }

    #[test]
    fn strategy_gas_matches_documented_costs() {
        use IRMemoryIOByteSize as S;
        for n in 1..=5 {
            assert_eq!(emit_set_with_refs(S::B32, n).set_gas, 15 * n - 6, "mstore, n={n}");
            assert_eq!(emit_set_with_refs(S::B1, n).set_gas, 15 * n - 6, "mstore8, n={n}");
            let or_merge = if n == 1 { 38 } else { 21 * n + 12 };
            assert_eq!(emit_set_with_refs(S::B7, n).set_gas, or_merge, "or-merge, n={n}");
            assert!(or_merge <= 27 * n + 11, "or-merge + mcopy no worse than or-merge each");
        }
        assert_eq!(emit_set_with_refs(S::B7, 0).set_gas, 4);
    }

    #[test]
    fn strategy_sizes() {
        use IRMemoryIOByteSize as S;
        // Offsets are < 256 here, so each offset push is 2 bytes.
        for n in 1..=5 {
            let n_usize = n as usize;
            assert_eq!(emit_set_with_refs(S::B32, n).set_bytes, 6 * n_usize - 2, "mstore, n={n}");
            let or_merge = if n == 1 { 15 } else { 8 * n_usize + 5 };
            assert_eq!(emit_set_with_refs(S::B7, n).set_bytes, or_merge, "or-merge, n={n}");
        }
    }

    fn assert_set_asm(size: IRMemoryIOByteSize, ref_count: u32, expected: &str) {
        pretty_assertions::assert_str_eq!(
            plank_test_utils::dedent_preserve_indent(&emit_set_with_refs(size, ref_count).asm),
            plank_test_utils::dedent_preserve_indent(expected)
        );
    }

    #[test]
    fn discard_unreferenced() {
        assert_set_asm(
            IRMemoryIOByteSize::B7,
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
            IRMemoryIOByteSize::B32,
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
            .mark1:
              data 0x0000000000000000000000000000000000000000000000000000000000000000 (32 bytes)
              POP
              PUSH32 0x (truncated)
            .mark2:
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
            IRMemoryIOByteSize::B1,
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
            .mark1:
              data 0x00 (1 bytes)
              POP
              PUSH1 0x (truncated)
            .mark2:
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
    fn or_merge_single_reference() {
        assert_set_asm(
            IRMemoryIOByteSize::B20,
            1,
            r#"
              SWAP1
              PUSH1 0x60
              SHL
              SWAP1
              PUSH (.mark1 - .mark0)
              ADD
              DUP1
              MLOAD
              DUP3
              OR
              SWAP1
              MSTORE
              POP
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
    fn or_merge_then_mcopy() {
        assert_set_asm(
            IRMemoryIOByteSize::B7,
            3,
            r#"
              SWAP1
              PUSH1 0xc8
              SHL
              DUP2
              PUSH (.mark1 - .mark0)
              ADD
              SWAP1
              DUP2
              MLOAD
              OR
              DUP2
              MSTORE
              PUSH1 0x07
              DUP2
              DUP4
              PUSH (.mark2 - .mark0)
              ADD
              MCOPY
              PUSH1 0x07
              SWAP2
              PUSH (.mark3 - .mark0)
              ADD
              MCOPY
            .mark0:
              PUSH7 0x (truncated)
            .mark1:
              data 0x00000000000000 (7 bytes)
              POP
              PUSH7 0x (truncated)
            .mark2:
              data 0x00000000000000 (7 bytes)
              POP
              PUSH7 0x (truncated)
            .mark3:
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
