use std::fmt;

use plank_core::IndexVec;

use crate::{CodeBlockId, DecodedBytecode, InstructionId, Opcode, icall::InternalCallInference};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockControl {
    Fallthrough(CodeBlockId),
    Goto(CodeBlockId),
    Branch { non_zero: CodeBlockId, zero: CodeBlockId },
    InternalReturn,
    Terminates,
    EndOfCode,
    InvalidJump,
    UnresolvedJump,
    UnresolvedJumpI,
    UnresolvedCall,
}

impl BlockControl {
    pub fn successors(self) -> impl Iterator<Item = CodeBlockId> {
        let (first, second) = match self {
            Self::Fallthrough(target) | Self::Goto(target) => (Some(target), None),
            Self::Branch { non_zero, zero } => (Some(non_zero), Some(zero)),
            Self::InternalReturn
            | Self::Terminates
            | Self::EndOfCode
            | Self::InvalidJump
            | Self::UnresolvedJump
            | Self::UnresolvedJumpI
            | Self::UnresolvedCall => (None, None),
        };
        first.into_iter().chain(second)
    }
}

#[derive(Debug, Clone)]
pub struct ProvisionalCfg {
    controls: IndexVec<CodeBlockId, BlockControl>,
    predecessors: IndexVec<CodeBlockId, Vec<CodeBlockId>>,
    calls: IndexVec<CodeBlockId, Vec<InstructionId>>,
}

pub fn build_provisional_cfg(
    decoded: &DecodedBytecode,
    inference: &InternalCallInference,
) -> ProvisionalCfg {
    let blocks = inference.code_blocks();
    let mut controls = IndexVec::with_capacity(blocks.blocks().len());
    let mut calls = IndexVec::with_capacity(blocks.blocks().len());

    for (block_id, block) in blocks.blocks().enumerate_idx() {
        calls.push(
            block
                .instructions
                .iter()
                .filter(|&instruction| inference.call(instruction).is_some())
                .collect(),
        );
        let last_id = block.instructions.end - 1;
        let last = decoded.instruction(last_id);
        let next = blocks.blocks().get(block_id + 1).map(|_| block_id + 1);
        let control = match last.op {
            Err(_) => BlockControl::Terminates,
            Ok(op) if op.is_terminating() => BlockControl::Terminates,
            Ok(Opcode::Jump) => {
                if let Some(call) = inference.call(last_id) {
                    if call.continuation_pc.is_none() {
                        BlockControl::UnresolvedCall
                    } else {
                        unreachable!("a call with a continuation cannot end its call-aware block")
                    }
                } else if inference.is_internal_return(last_id) {
                    BlockControl::InternalReturn
                } else if let Some(destination) = inference.static_jump(last_id) {
                    blocks
                        .jumpdest_block(destination.pc)
                        .map_or(BlockControl::InvalidJump, BlockControl::Goto)
                } else {
                    BlockControl::UnresolvedJump
                }
            }
            Ok(Opcode::JumpI) => match (inference.static_jump(last_id), next) {
                (Some(destination), Some(zero)) => blocks.jumpdest_block(destination.pc).map_or(
                    BlockControl::InvalidJump,
                    |non_zero| BlockControl::Branch { non_zero, zero },
                ),
                (Some(destination), None) => blocks
                    .jumpdest_block(destination.pc)
                    .map_or(BlockControl::InvalidJump, |_| BlockControl::UnresolvedJumpI),
                (None, _) => BlockControl::UnresolvedJumpI,
            },
            _ => next.map_or(BlockControl::EndOfCode, BlockControl::Fallthrough),
        };
        controls.push(control);
    }

    let mut predecessors = IndexVec::from_vec(vec![Vec::new(); blocks.blocks().len()]);
    for (source, &control) in controls.enumerate_idx() {
        for target in control.successors() {
            predecessors[target].push(source);
        }
    }

    ProvisionalCfg { controls, predecessors, calls }
}

impl ProvisionalCfg {
    pub fn control(&self, block: CodeBlockId) -> BlockControl {
        self.controls[block]
    }

    pub fn predecessors(&self, block: CodeBlockId) -> &[CodeBlockId] {
        &self.predecessors[block]
    }

    pub fn calls(&self, block: CodeBlockId) -> &[InstructionId] {
        &self.calls[block]
    }

    pub fn display<'a>(
        &'a self,
        decoded: &'a DecodedBytecode,
        inference: &'a InternalCallInference,
    ) -> CfgDisplay<'a> {
        CfgDisplay { decoded, inference, cfg: self }
    }
}

pub struct CfgDisplay<'a> {
    decoded: &'a DecodedBytecode,
    inference: &'a InternalCallInference,
    cfg: &'a ProvisionalCfg,
}

impl fmt::Display for CfgDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (block_id, block) in self.inference.code_blocks().blocks().enumerate_idx() {
            write!(f, "@{block_id} [0x{:x},0x{:x})", block.start_pc, block.end_pc)?;
            for &call in self.cfg.calls(block_id) {
                let call = self.inference.call(call).expect("recorded call should exist");
                write!(f, " call(f{} -> 0x{:x})", call.function, call.destination_pc)?;
            }
            write!(f, " => ")?;
            match self.cfg.control(block_id) {
                BlockControl::Fallthrough(target) => write!(f, "fallthrough @{target}")?,
                BlockControl::Goto(target) => write!(f, "goto @{target}")?,
                BlockControl::Branch { non_zero, zero } => {
                    write!(f, "branch @{non_zero} else @{zero}")?
                }
                BlockControl::InternalReturn => write!(f, "iret")?,
                BlockControl::Terminates => write!(f, "terminates")?,
                BlockControl::EndOfCode => write!(f, "end-of-code")?,
                BlockControl::InvalidJump => write!(f, "invalid-jump")?,
                BlockControl::UnresolvedJump => write!(f, "unresolved-jump")?,
                BlockControl::UnresolvedJumpI => write!(f, "unresolved-jumpi")?,
                BlockControl::UnresolvedCall => write!(f, "unresolved-call")?,
            }
            if let Some(last) = block.instructions.iter().next_back() {
                write!(f, " ; last=#{} pc=0x{:x}", last, self.decoded.instruction(last).pc)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}
