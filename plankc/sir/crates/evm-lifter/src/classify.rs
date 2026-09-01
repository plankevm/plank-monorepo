use std::{fmt, ops::Range};

use plank_core::IndexVec;

use crate::{
    CodeBlockId, DataSectionId, DecodedBytecode, icall::InternalCallInference, ownership::Ownership,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSection {
    pub bytes: Range<u32>,
}

#[derive(Debug, Clone)]
pub struct ClassifiedProgram {
    code: IndexVec<CodeBlockId, bool>,
    data: IndexVec<DataSectionId, DataSection>,
}

pub fn classify(
    decoded: &DecodedBytecode,
    inference: &InternalCallInference,
    ownership: &Ownership,
) -> ClassifiedProgram {
    let code = IndexVec::from_vec(
        inference.code_blocks().blocks().iter_idx().map(|block| ownership.is_code(block)).collect(),
    );
    let mut data = IndexVec::<DataSectionId, DataSection>::new();
    for (block, code_block) in inference.code_blocks().blocks().enumerate_idx() {
        if code[block] {
            continue;
        }
        if let Some(last) = data.raw.last_mut()
            && last.bytes.end == code_block.start_pc
        {
            last.bytes.end = code_block.end_pc;
        } else {
            data.push(DataSection { bytes: code_block.start_pc..code_block.end_pc });
        }
    }
    if let Some(last) = data.raw.last_mut() {
        last.bytes.end = last.bytes.end.min(decoded.bytes().len() as u32);
    }
    ClassifiedProgram { code, data }
}

impl ClassifiedProgram {
    pub fn is_code(&self, block: CodeBlockId) -> bool {
        self.code[block]
    }

    pub fn data_sections(&self) -> &IndexVec<DataSectionId, DataSection> {
        &self.data
    }

    pub fn display<'a>(&'a self, decoded: &'a DecodedBytecode) -> ClassifiedDisplay<'a> {
        ClassifiedDisplay { classified: self, decoded }
    }
}

pub struct ClassifiedDisplay<'a> {
    classified: &'a ClassifiedProgram,
    decoded: &'a DecodedBytecode,
}

impl fmt::Display for ClassifiedDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.classified.data.is_empty() {
            return writeln!(f, "no data sections");
        }
        for (id, section) in self.classified.data.enumerate_idx() {
            write!(f, ".d{id} [0x{:x},0x{:x}) 0x", section.bytes.start, section.bytes.end)?;
            for byte in
                &self.decoded.bytes()[section.bytes.start as usize..section.bytes.end as usize]
            {
                write!(f, "{byte:02x}")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}
