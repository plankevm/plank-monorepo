pub mod cfg;
pub mod classify;
pub mod decode;
pub mod icall;
pub mod lower;
pub mod opcode;
pub mod ownership;
pub mod primitive_blocks;
pub mod ssa;
pub mod symbolic;
pub mod verify;

use plank_core::newtype_index;

newtype_index! {
    pub struct InstructionId;
    pub struct PrimitiveBlockId;
    pub struct CodeBlockId;
    pub struct FunctionCandidateId;
    pub struct SymbolicValueId;
    pub struct SsaValueId;
    pub struct SsaBlockId;
    pub struct DataSectionId;
}

pub use decode::{DecodedBytecode, Instruction, decode};
pub use opcode::{Opcode, StackIO};
