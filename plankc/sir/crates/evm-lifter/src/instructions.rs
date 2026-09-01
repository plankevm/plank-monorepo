use alloy_primitives::U256;

use crate::Opcode;

pub struct Instructions<'b> {
    bytes: &'b [u8],
    instruction_index_to_byte: Box<[u32]>,
}

#[derive(Debug, Clone, Copy)]
pub struct InstructionView<'b> {
    pc: u32,
    op: Result<Opcode, u8>,
    raw_immediate: Option<&'b [u8]>,
}

impl<'b> InstructionView<'b> {
    pub fn pc(self) -> u32 {
        self.pc
    }

    pub fn op(self) -> Result<Opcode, u8> {
        self.op
    }

    pub const fn immediate_size(self) -> Option<u8> {
        let Ok(op) = self.op else { return None };
        Opcode::immediate_bytes(op)
    }

    pub fn immediate(self) -> Option<U256> {
        let (size, raw) = self.immediate_size().map(usize::from).zip(self.raw_immediate)?;
        let mut buf = [0u8; 32];
        buf[32 - size..size.min(raw.len())].copy_from_slice(raw);
        Some(U256::from_be_bytes(buf))
    }
}

const AVG_BYTES_PER_INSTRUCTION: usize = 2;

pub fn decode<'b>(bytes: &'b [u8], pc: u32) -> InstructionView<'b> {
    let byte = bytes[pc as usize];
    let op = Opcode::from_byte(byte);
    let raw_immediate = op.and_then(|op| op.immediate_bytes()).map(|imm| {
        let remaining = &bytes[pc as usize + 1..];
        &remaining[..remaining.len().min(usize::from(imm))]
    });
    InstructionView { pc, op: op.ok_or(byte), raw_immediate }
}

impl<'b> Instructions<'b> {
    pub fn new(bytes: &'b [u8]) -> Self {
        let mut instruction_index_to_bytes =
            Vec::with_capacity(bytes.len().div_ceil(AVG_BYTES_PER_INSTRUCTION));

        let mut pc = 0u32;
        while (pc as usize) < bytes.len() {
            instruction_index_to_bytes.push(pc);
            if let Some(immediate_bytes) =
                Opcode::from_byte(bytes[pc as usize]).and_then(Opcode::immediate_bytes)
            {
                pc += u32::from(immediate_bytes);
            }
            pc += 1;
        }

        Self { bytes, instruction_index_to_byte: instruction_index_to_bytes.into() }
    }

    pub fn pc_to_instruction_index(&self, pc: u32) -> Result<usize, usize> {
        self.instruction_index_to_byte.binary_search(&pc).map_err(|i| i.saturating_sub(1))
    }

    pub fn instruction(&self, i: usize) -> InstructionView<'b> {
        decode(self.bytes, self.instruction_index_to_byte[i])
    }

    pub fn total(&self) -> u32 {
        self.instruction_index_to_byte.len().try_into().expect("overflow")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_instruction_starts_around_push_immediates() {
        let bytes = [0x60, 0xaa, 0x01, 0x61, 0xbb];
        let instructions = Instructions::new(&bytes);

        assert_eq!(&*instructions.instruction_index_to_byte, &[0, 2, 3]);
        assert_eq!(instructions.pc_to_instruction_index(0), Ok(0));
        assert_eq!(instructions.pc_to_instruction_index(1), Err(0));
        assert_eq!(instructions.pc_to_instruction_index(2), Ok(1));
        assert_eq!(instructions.pc_to_instruction_index(3), Ok(2));
        assert_eq!(instructions.pc_to_instruction_index(4), Err(2));
    }

    #[test]
    fn views_include_opcode_and_available_immediate_bytes() {
        let bytes = [0x60, 0xaa, 0x01, 0x61, 0xbb];
        let instructions = Instructions::new(&bytes);

        let push1 = instructions.instruction(0);
        assert_eq!(push1.pc, 0);
        assert_eq!(push1.op, Ok(Opcode::Push1));
        assert_eq!(push1.raw_immediate, Some(&bytes[1..2]));

        let add = instructions.instruction(1);
        assert_eq!(add.pc, 2);
        assert_eq!(add.op, Ok(Opcode::Add));
        assert_eq!(add.raw_immediate, None);

        let truncated_push2 = instructions.instruction(2);
        assert_eq!(truncated_push2.pc, 3);
        assert_eq!(truncated_push2.op, Ok(Opcode::Push2));
        assert_eq!(truncated_push2.raw_immediate, Some(&bytes[4..5]));
    }

    #[test]
    fn unknown_bytes_are_instructions_without_immediates() {
        let bytes = [0xab, 0x60];
        let instructions = Instructions::new(&bytes);

        assert_eq!(&*instructions.instruction_index_to_byte, &[0, 1]);

        let unknown = instructions.instruction(0);
        assert_eq!(unknown.pc, 0);
        assert_eq!(unknown.op, Err(0xab));
        assert_eq!(unknown.raw_immediate, None);

        let missing_immediate = instructions.instruction(1);
        assert_eq!(missing_immediate.pc, 1);
        assert_eq!(missing_immediate.op, Ok(Opcode::Push1));
        assert_eq!(missing_immediate.raw_immediate, Some(&bytes[2..]));
    }
}
