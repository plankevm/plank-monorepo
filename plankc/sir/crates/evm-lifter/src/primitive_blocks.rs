use plank_core::{Idx, IndexVec, Span, newtype_index};

use crate::{Opcode, instructions::Instructions};
newtype_index! {
    pub struct PrimitiveBlockId;
}

struct Block {
    instructions_start: u32,
}

pub struct PrimitiveBlocks {
    blocks: IndexVec<PrimitiveBlockId, Block>,
    total_instructions: u32,
}

const AVG_INSTRUCTIONS_PER_BLOCK: usize = 4;

impl PrimitiveBlocks {
    pub fn new(instructions: Instructions<'_>) -> PrimitiveBlocks {
        let mut blocks = IndexVec::with_capacity(
            (instructions.total() as usize).div_ceil(AVG_INSTRUCTIONS_PER_BLOCK),
        );

        let mut start = 0;
        let mut flush_block = |start| blocks.push(Block { instructions_start: start });

        for i in 0..instructions.total() {
            let instr = instructions.instruction(i as usize);
            if matches!(instr.op(), Ok(Opcode::JumpDest)) && start < i {
                flush_block(start);
                start = i;
            } else if instr.op().is_ok_and(Opcode::is_terminating)
                || matches!(instr.op(), Ok(Opcode::Jump | Opcode::JumpI) | Err(_))
            {
                flush_block(start);
                start = i + 1;
            }
        }

        if start < instructions.total() {
            flush_block(start);
        }

        PrimitiveBlocks { blocks, total_instructions: instructions.total() }
    }

    #[track_caller]
    pub fn instructions(&self, block: PrimitiveBlockId) -> Span<u32> {
        let start = self.blocks[block].instructions_start;
        let end = block
            .checked_add(1)
            .and_then(|next| self.blocks.get(next))
            .map_or(self.total_instructions, |b: &Block| b.instructions_start);

        Span::new(start, end)
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::*;

    fn spans(bytes: impl AsRef<[u8]>) -> Vec<Range<u32>> {
        let instructions = Instructions::new(bytes.as_ref());
        let blocks = PrimitiveBlocks::new(instructions);

        (0..blocks.blocks.len())
            .map(|index| blocks.instructions(PrimitiveBlockId::new(index as u32)).range())
            .collect()
    }

    #[test]
    fn splits_after_jumps_and_terminators() {
        assert_eq!(
            spans(bytecode![Add, Jump, Add, JumpI, Add, Stop, Add]),
            vec![0..2, 2..4, 4..6, 6..7],
        );
    }

    #[test]
    fn splits_before_jumpdest_after_existing_instructions() {
        assert_eq!(spans(bytecode![Add, JumpDest, Add, JumpDest, Stop]), vec![0..1, 1..3, 3..5],);
    }

    #[test]
    fn unknown_opcodes_end_a_block() {
        assert_eq!(spans(bytecode![Add, 0xab, Add]), vec![0..2, 2..3]);
    }

    #[test]
    fn does_not_create_empty_blocks() {
        assert_eq!(spans(bytecode![]), Vec::<Range<u32>>::new());
        assert_eq!(spans(bytecode![Stop, Add]), vec![0..1, 1..2]);
        assert_eq!(spans(bytecode![JumpDest, Add]), vec![0..2]);
    }

    #[test]
    fn jumpdest_after_boundary_does_not_duplicate_or_empty_split() {
        assert_eq!(spans(bytecode![Stop, JumpDest]), vec![0..1, 1..2]);
        assert_eq!(spans(bytecode![Return, JumpDest]), vec![0..1, 1..2]);
        assert_eq!(spans(bytecode![Revert, JumpDest]), vec![0..1, 1..2]);
        assert_eq!(spans(bytecode![Invalid, JumpDest]), vec![0..1, 1..2]);
        assert_eq!(spans(bytecode![SelfDestruct, JumpDest]), vec![0..1, 1..2]);
        assert_eq!(spans(bytecode![Jump, JumpDest]), vec![0..1, 1..2]);
        assert_eq!(spans(bytecode![JumpI, JumpDest]), vec![0..1, 1..2]);
        assert_eq!(spans(bytecode![Stop, JumpDest, JumpDest]), vec![0..1, 1..2, 2..3]);
    }
}
