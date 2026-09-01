use std::fmt;

use alloy_primitives::U256;
use plank_core::{Idx, IndexVec, Span};

use crate::{InstructionId, Opcode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub op: Result<Opcode, u8>,
    pub pc: u32,
    immediate: [u8; 32],
    immediate_len: u8,
    available_immediate_len: u8,
    actual_byte_size: u8,
}

impl Instruction {
    pub fn immediate(&self) -> Option<&[u8]> {
        (self.immediate_len != 0).then_some(&self.immediate[..self.immediate_len as usize])
    }

    pub fn available_immediate(&self) -> Option<&[u8]> {
        (self.immediate_len != 0)
            .then_some(&self.immediate[..self.available_immediate_len as usize])
    }

    pub const fn declared_immediate_len(&self) -> u8 {
        self.immediate_len
    }

    pub const fn available_immediate_len(&self) -> u8 {
        self.available_immediate_len
    }

    pub const fn encoded_byte_size(&self) -> u8 {
        self.immediate_len + 1
    }

    pub const fn actual_byte_size(&self) -> u8 {
        self.actual_byte_size
    }

    pub fn immediate_as_u32(&self) -> Option<u32> {
        let immediate = self.immediate()?;
        let first_value_byte = immediate.len().saturating_sub(4);
        if immediate[..first_value_byte].iter().any(|&byte| byte != 0) {
            return None;
        }
        Some(
            immediate[first_value_byte..]
                .iter()
                .fold(0, |value, &byte| (value << 8) | u32::from(byte)),
        )
    }

    pub fn immediate_as_u256(&self) -> Option<U256> {
        self.immediate().map(U256::from_be_slice)
    }

    pub fn actual_byte_range(&self) -> std::ops::Range<u32> {
        self.pc..self.pc + u32::from(self.actual_byte_size)
    }
}

#[derive(Debug, Clone)]
pub struct DecodedBytecode {
    bytes: Box<[u8]>,
    instructions: IndexVec<InstructionId, Instruction>,
    instruction_at_pc: Vec<Option<InstructionId>>,
}

pub fn decode(bytecode: &[u8]) -> Result<DecodedBytecode, DecodeError> {
    let bytecode_len = u32::try_from(bytecode.len()).map_err(|_| DecodeError::BytecodeTooLarge)?;
    if bytecode.is_empty() {
        return Err(DecodeError::EmptyBytecode);
    }

    let mut instructions = IndexVec::with_capacity(bytecode.len());
    let mut instruction_at_pc = vec![None; bytecode.len()];
    let mut pc = 0u32;
    while pc < bytecode_len {
        let byte = bytecode[pc as usize];
        let op = Opcode::from_byte(byte);
        let immediate_len = op.and_then(Opcode::push_size).unwrap_or(0);
        let available_immediate_len =
            u8::try_from((bytecode.len() - pc as usize - 1).min(immediate_len as usize))
                .expect("PUSH immediate length fits u8");
        let mut immediate = [0; 32];
        let immediate_start = pc as usize + 1;
        let immediate_end = immediate_start + available_immediate_len as usize;
        immediate[..available_immediate_len as usize]
            .copy_from_slice(&bytecode[immediate_start..immediate_end]);
        let actual_byte_size = available_immediate_len + 1;
        let id = instructions.push(Instruction {
            op: op.ok_or(byte),
            pc,
            immediate,
            immediate_len,
            available_immediate_len,
            actual_byte_size,
        });
        instruction_at_pc[pc as usize] = Some(id);
        pc = pc.saturating_add(u32::from(immediate_len) + 1).min(bytecode_len);
    }

    Ok(DecodedBytecode { bytes: bytecode.into(), instructions, instruction_at_pc })
}

impl DecodedBytecode {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn instructions(&self) -> &IndexVec<InstructionId, Instruction> {
        &self.instructions
    }

    pub fn instruction(&self, id: InstructionId) -> &Instruction {
        &self.instructions[id]
    }

    pub fn instruction_at_pc(&self, pc: u32) -> Option<InstructionId> {
        self.instruction_at_pc.get(pc as usize).copied().flatten()
    }

    pub fn instruction_span(&self) -> Span<InstructionId> {
        Span::new(InstructionId::ZERO, self.instructions.len_idx())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("bytecode is empty")]
    EmptyBytecode,
    #[error("bytecode exceeds the 32-bit EVM PC range")]
    BytecodeTooLarge,
}

impl fmt::Display for DecodedBytecode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (id, instruction) in self.instructions.enumerate_idx() {
            write!(f, "#{id} {:08x}: ", instruction.pc)?;
            match instruction.op {
                Ok(op) => write!(f, "{op}")?,
                Err(byte) => write!(f, "UNKNOWN(0x{byte:02X})")?,
            }
            if let Some(immediate) = instruction.immediate() {
                write!(f, " 0x")?;
                for byte in immediate {
                    write!(f, "{byte:02x}")?;
                }
                if instruction.available_immediate_len < instruction.immediate_len {
                    write!(
                        f,
                        " ({} of {} immediate bytes present)",
                        instruction.available_immediate_len, instruction.immediate_len
                    )?;
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_truncated_push_with_zero_padding() {
        let decoded = decode(&[Opcode::Push3 as u8, 0xaa]).unwrap();
        let actual = decoded.to_string();
        let expected = r#"
            #0 00000000: PUSH3 0xaa0000 (1 of 3 immediate bytes present)
        "#;
        pretty_assertions::assert_str_eq!(
            plank_test_utils::dedent_preserve_indent(&actual),
            plank_test_utils::dedent_preserve_indent(expected),
        );
        let instruction = &decoded.instructions[InstructionId::ZERO];
        assert_eq!(instruction.available_immediate(), Some([0xaa].as_slice()));
        assert_eq!(instruction.actual_byte_range(), 0..2);
    }
}
