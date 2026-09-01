use crate::{
    Opcode,
    abs_evm::{AbstractStack, Control, EvmError, Value},
    instructions::Instructions,
};
use plank_core::{Idx, IndexVec, Span, newtype_index};

newtype_index! {
    pub struct PrimitiveBlockId;
}

#[derive(Debug, Clone, Copy)]
struct StoredBlock {
    instructions_start: u32,
    terminator: Option<Terminator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminator {
    Terminates,
    IllegalJump,
    PotentialInternalReturn,
    JumpsTo(u32),
    JumpIfTo(u32),
}

pub struct Block {
    pub instructions: Span<u32>,
    pub terminator: Option<Terminator>,
}

pub struct PrimitiveBlocks {
    blocks: IndexVec<PrimitiveBlockId, StoredBlock>,
    total_instructions: u32,
}

const AVG_INSTRUCTIONS_PER_BLOCK: usize = 4;

impl PrimitiveBlocks {
    pub fn new(instructions: Instructions<'_>) -> PrimitiveBlocks {
        let mut blocks = IndexVec::with_capacity(
            (instructions.total() as usize).div_ceil(AVG_INSTRUCTIONS_PER_BLOCK),
        );

        let mut stack = AbstractStack::new();
        let mut start = 0;
        let mut flush_block =
            |start, terminator| blocks.push(StoredBlock { instructions_start: start, terminator });

        for i in 0..instructions.total() {
            let instr = instructions.instruction(i);
            if i == start {
                stack.clear();
            }

            if matches!(instr.op, Ok(Opcode::JumpDest)) && start < i {
                flush_block(start, None);
                start = i;
                stack.clear();

                assert_eq!(stack.execute(instr), Ok(Control::Step));
            } else {
                let control = stack.execute(instr);
                match control {
                    Ok(Control::JumpTo(v)) => {
                        let terminator = match v {
                            Value::Constant(x) => match u32::try_from(x) {
                                Ok(pc) => Terminator::JumpsTo(pc),
                                Err(_) => Terminator::Terminates,
                            },
                            Value::FunctionInput(_) => Terminator::PotentialInternalReturn,
                            Value::Symbolic => Terminator::IllegalJump,
                        };
                        flush_block(start, Some(terminator));
                        start = i + 1;
                    }
                    Ok(Control::JumpIfUnknownTo(v)) => {
                        let terminator = match v {
                            Value::Constant(x) => match u32::try_from(x) {
                                Ok(pc) => Terminator::JumpIfTo(pc),
                                Err(_) => Terminator::Terminates,
                            },
                            _ => Terminator::IllegalJump,
                        };
                        flush_block(start, Some(terminator));
                        start = i + 1;
                    }
                    Ok(Control::Terminate) | Err(EvmError::StackOverflow) => {
                        flush_block(start, Some(Terminator::Terminates));
                        start = i + 1;
                    }
                    Ok(Control::Step) => {}
                }
            }
        }

        if start < instructions.total() {
            flush_block(start, Some(Terminator::Terminates));
        }

        for id in blocks.iter_idx() {
            let block: StoredBlock = blocks[id];
            match block.terminator {
                Some(Terminator::JumpsTo(pc)) => {
                    (&mut blocks[id] as &mut StoredBlock).terminator =
                        instructions.jumpdest(pc).map_or(Some(Terminator::Terminates), |i| {
                            let dst_block = blocks
                                .binary_search_by_key(&i, |b| b.instructions_start)
                                .expect("jumpdest is not start of any block");
                            Some(Terminator::JumpsTo(dst_block.try_into().expect("overflow")))
                        });
                }
                Some(Terminator::JumpIfTo(pc)) => {
                    (&mut blocks[id] as &mut StoredBlock).terminator =
                        instructions.jumpdest(pc).map_or(Some(Terminator::Terminates), |i| {
                            let dst_block = blocks
                                .binary_search_by_key(&i, |b| b.instructions_start)
                                .expect("jumpdest is not start of any block");
                            Some(Terminator::JumpIfTo(dst_block.try_into().expect("overflow")))
                        });
                }
                None
                | Some(
                    Terminator::Terminates
                    | Terminator::IllegalJump
                    | Terminator::PotentialInternalReturn,
                ) => {}
            }
        }

        PrimitiveBlocks { blocks, total_instructions: instructions.total() }
    }

    #[track_caller]
    pub fn get_block(&self, id: PrimitiveBlockId) -> Block {
        let block = self.blocks[id];
        let end = id
            .checked_add(1)
            .and_then(|next| self.blocks.get(next))
            .map_or(self.total_instructions, |b: &StoredBlock| b.instructions_start);

        Block {
            instructions: Span::new(block.instructions_start, end),
            terminator: block.terminator,
        }
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
            .map(|index| blocks.get_block(PrimitiveBlockId::new(index as u32)).instructions.range())
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

    #[test]
    fn resolves_direct_jumps_to_jumpdest_blocks() {
        let bytes = bytecode![Push1, 0x03, Jump, JumpDest, Stop];
        let instructions = Instructions::new(&bytes);
        let blocks = PrimitiveBlocks::new(instructions);
        let block = blocks.get_block(PrimitiveBlockId::new(0));

        assert_eq!(block.instructions.range(), 0..2);
        assert_eq!(block.terminator, Some(Terminator::JumpsTo(1)));

        let bytes = bytecode![CallValue, Push1, 0x06, JumpI, JumpDest, Invalid, JumpDest, Stop];
        let instructions = Instructions::new(&bytes);
        let blocks = PrimitiveBlocks::new(instructions);
        let block = blocks.get_block(PrimitiveBlockId::new(0));

        assert_eq!(block.instructions.range(), 0..3);
        assert!(matches!(block.terminator, Some(Terminator::JumpIfTo(2))));
    }
}
