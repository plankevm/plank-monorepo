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
