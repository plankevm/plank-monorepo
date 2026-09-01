use std::{collections::BTreeMap, fmt};

use plank_core::{Idx, IndexVec, Span};

use crate::{DecodedBytecode, InstructionId, Opcode, PrimitiveBlockId, StackIO};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticJumpDestination {
    pub push: InstructionId,
    pub pc: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AbstractValue {
    Unknown,
    Constant { push: InstructionId, value: alloy_primitives::U256 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrimitiveBlock {
    pub instructions: Span<InstructionId>,
    pub start_pc: u32,
    pub end_pc: u32,
    pub stack_io: StackIO,
}

#[derive(Debug, Clone)]
pub struct PrimitiveBlocks {
    blocks: IndexVec<PrimitiveBlockId, PrimitiveBlock>,
    jumpdestinations: BTreeMap<u32, PrimitiveBlockId>,
    static_jumps: IndexVec<InstructionId, Option<StaticJumpDestination>>,
}

pub fn build_primitive_blocks(decoded: &DecodedBytecode) -> PrimitiveBlocks {
    let instructions = decoded.instructions();
    let mut ranges = Vec::with_capacity(instructions.len() / 4);
    let mut start = InstructionId::ZERO;

    for (id, instruction) in instructions.enumerate_idx() {
        if instruction.op == Ok(Opcode::JumpDest) && start < id {
            ranges.push(Span::new(start, id));
            start = id;
        }

        if matches!(instruction.op, Ok(Opcode::Jump | Opcode::JumpI) | Err(_))
            || instruction.op.is_ok_and(Opcode::is_terminating)
        {
            ranges.push(Span::new(start, id + 1));
            start = id + 1;
        }
    }
    if start < instructions.len_idx() {
        ranges.push(Span::new(start, instructions.len_idx()));
    }

    let mut blocks = IndexVec::with_capacity(ranges.len());
    let mut jumpdestinations = BTreeMap::new();
    let mut static_jumps = IndexVec::from_vec(vec![None; instructions.len()]);
    for range in ranges {
        let first = &instructions[range.start];
        let last = &instructions[range.end - 1];
        let stack_io = instructions[range].iter().fold(StackIO::default(), |io, instruction| {
            io.chain(instruction.op.map_or(StackIO::default(), Opcode::stack_io))
        });
        let block_id = blocks.push(PrimitiveBlock {
            instructions: range,
            start_pc: first.pc,
            end_pc: last.actual_byte_range().end,
            stack_io,
        });
        if first.op == Ok(Opcode::JumpDest) {
            jumpdestinations.insert(first.pc, block_id);
        }
        interpret_static_jump(decoded, range, &mut static_jumps);
    }

    PrimitiveBlocks { blocks, jumpdestinations, static_jumps }
}

fn interpret_static_jump(
    decoded: &DecodedBytecode,
    instructions: Span<InstructionId>,
    static_jumps: &mut IndexVec<InstructionId, Option<StaticJumpDestination>>,
) {
    let mut stack = Vec::new();
    for instruction_id in instructions.iter() {
        let instruction = decoded.instruction(instruction_id);
        let Ok(opcode) = instruction.op else { return };
        if opcode.is_push() || opcode == Opcode::Push0 {
            let value = if opcode == Opcode::Push0 {
                alloy_primitives::U256::ZERO
            } else {
                instruction.immediate_as_u256().expect("PUSH should have an immediate")
            };
            stack.insert(0, AbstractValue::Constant { push: instruction_id, value });
        } else if opcode == Opcode::Pop {
            pop_abstract(&mut stack);
        } else if let Some(depth) = opcode.is_dup() {
            ensure_abstract_depth(&mut stack, depth as usize);
            let value = stack[depth as usize - 1];
            stack.insert(0, value);
        } else if let Some(depth) = opcode.is_swap() {
            ensure_abstract_depth(&mut stack, depth as usize + 1);
            stack.swap(0, depth as usize);
        } else if matches!(opcode, Opcode::Jump | Opcode::JumpI) {
            let destination = pop_abstract(&mut stack);
            if let AbstractValue::Constant { push, value } = destination
                && value <= alloy_primitives::U256::from(u32::MAX)
            {
                static_jumps[instruction_id] =
                    Some(StaticJumpDestination { push, pc: value.to::<u32>() });
            }
            if opcode == Opcode::JumpI {
                pop_abstract(&mut stack);
            }
        } else if opcode != Opcode::JumpDest {
            let io = opcode.stack_io();
            for _ in 0..io.inputs {
                pop_abstract(&mut stack);
            }
            for _ in 0..io.outputs {
                stack.insert(0, AbstractValue::Unknown);
            }
        }
    }
}

fn ensure_abstract_depth(stack: &mut Vec<AbstractValue>, depth: usize) {
    stack.resize(depth, AbstractValue::Unknown);
}

fn pop_abstract(stack: &mut Vec<AbstractValue>) -> AbstractValue {
    if stack.is_empty() { AbstractValue::Unknown } else { stack.remove(0) }
}

impl PrimitiveBlocks {
    pub fn blocks(&self) -> &IndexVec<PrimitiveBlockId, PrimitiveBlock> {
        &self.blocks
    }

    pub fn block(&self, id: PrimitiveBlockId) -> &PrimitiveBlock {
        &self.blocks[id]
    }

    pub fn jumpdest_block(&self, pc: u32) -> Option<PrimitiveBlockId> {
        self.jumpdestinations.get(&pc).copied()
    }

    pub fn static_jump(&self, instruction: InstructionId) -> Option<StaticJumpDestination> {
        self.static_jumps[instruction]
    }

    pub fn display<'a>(&'a self, decoded: &'a DecodedBytecode) -> PrimitiveBlocksDisplay<'a> {
        PrimitiveBlocksDisplay { decoded, blocks: self }
    }
}

pub struct PrimitiveBlocksDisplay<'a> {
    decoded: &'a DecodedBytecode,
    blocks: &'a PrimitiveBlocks,
}

impl fmt::Display for PrimitiveBlocksDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (block_id, block) in self.blocks.blocks.enumerate_idx() {
            writeln!(
                f,
                "@{block_id} pc=[0x{:x},0x{:x}) io=({}, {})",
                block.start_pc, block.end_pc, block.stack_io.inputs, block.stack_io.outputs
            )?;
            for instruction_id in block.instructions.iter() {
                let instruction = self.decoded.instruction(instruction_id);
                write!(f, "    #{instruction_id} {:08x}: ", instruction.pc)?;
                match instruction.op {
                    Ok(op) => write!(f, "{op}")?,
                    Err(byte) => write!(f, "UNKNOWN(0x{byte:02X})")?,
                }
                if let Some(immediate) = instruction.immediate() {
                    write!(f, " 0x")?;
                    for byte in immediate {
                        write!(f, "{byte:02x}")?;
                    }
                }
                if let Some(destination) = self.blocks.static_jump(instruction_id) {
                    write!(f, " ; direct=0x{:x} from #{}", destination.pc, destination.push)?;
                }
                writeln!(f)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode;

    #[test]
    fn resolves_jump_destination_through_stack_operations() {
        let bytes = alloy_primitives::hex::decode("610173905056").unwrap();
        let decoded = decode(&bytes).unwrap();
        let blocks = build_primitive_blocks(&decoded);
        let actual = blocks.display(&decoded).to_string();
        let expected = r#"
            @0 pc=[0x0,0x6) io=(1, 0)
                #0 00000000: PUSH2 0x0173
                #1 00000003: SWAP1
                #2 00000004: POP
                #3 00000005: JUMP ; direct=0x173 from #0
        "#;
        pretty_assertions::assert_str_eq!(
            plank_test_utils::dedent_preserve_indent(&actual),
            plank_test_utils::dedent_preserve_indent(expected),
        );
    }

    #[test]
    fn small_huff_primitive_blocks() {
        let bytes =
            alloy_primitives::hex::decode(include_str!("../tests/fixtures/small.hex").trim())
                .unwrap();
        let decoded = decode(&bytes).unwrap();
        let blocks = build_primitive_blocks(&decoded);
        let actual = blocks.display(&decoded).to_string();
        let expected = r#"
            @0 pc=[0x0,0x3) io=(0, 0)
                #0 00000000: PUSH1 0x06
                #1 00000002: JUMP ; direct=0x6 from #0
            @1 pc=[0x3,0x4) io=(0, 0)
                #2 00000003: STOP
            @2 pc=[0x4,0x5) io=(0, 0)
                #3 00000004: STOP
            @3 pc=[0x5,0x6) io=(0, 0)
                #4 00000005: STOP
            @4 pc=[0x6,0x7) io=(0, 0)
                #5 00000006: JUMPDEST
        "#;
        pretty_assertions::assert_str_eq!(
            plank_test_utils::dedent_preserve_indent(&actual),
            plank_test_utils::dedent_preserve_indent(expected),
        );
    }
}
