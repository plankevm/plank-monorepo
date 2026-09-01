use std::{collections::BTreeSet, fmt};

use plank_core::IndexVec;

use crate::{CodeBlockId, FunctionCandidateId, cfg::ProvisionalCfg, icall::InternalCallInference};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    Returning,
    Root,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FunctionCandidate {
    pub entry: CodeBlockId,
    pub kind: FunctionKind,
}

#[derive(Debug, Clone)]
pub struct Ownership {
    functions: IndexVec<FunctionCandidateId, FunctionCandidate>,
    root: FunctionCandidateId,
    owners: IndexVec<CodeBlockId, BTreeSet<FunctionCandidateId>>,
}

pub fn analyze_ownership(
    inference: &InternalCallInference,
    cfg: &ProvisionalCfg,
) -> Result<Ownership, OwnershipError> {
    let blocks = inference.code_blocks();
    let mut functions = IndexVec::with_capacity(inference.functions().len() + 1);
    for function in inference.functions().iter() {
        let entry = blocks
            .jumpdest_block(function.entry_pc)
            .ok_or(OwnershipError::MissingFunctionEntry { pc: function.entry_pc })?;
        functions.push(FunctionCandidate { entry, kind: FunctionKind::Returning });
    }
    let root = functions.push(FunctionCandidate {
        entry: blocks
            .blocks()
            .iter_idx()
            .next()
            .expect("decoded bytecode should contain a root block"),
        kind: FunctionKind::Root,
    });

    let mut owners = IndexVec::from_vec(vec![BTreeSet::new(); blocks.blocks().len()]);
    for (function, candidate) in functions.enumerate_idx() {
        let mut worklist = vec![candidate.entry];
        let mut visited = BTreeSet::new();
        while let Some(block) = worklist.pop() {
            if !visited.insert(block) {
                continue;
            }
            owners[block].insert(function);
            worklist.extend(cfg.control(block).successors());
        }
    }

    Ok(Ownership { functions, root, owners })
}

impl Ownership {
    pub fn functions(&self) -> &IndexVec<FunctionCandidateId, FunctionCandidate> {
        &self.functions
    }

    pub fn root(&self) -> FunctionCandidateId {
        self.root
    }

    pub fn owners(&self, block: CodeBlockId) -> &BTreeSet<FunctionCandidateId> {
        &self.owners[block]
    }

    pub fn is_owned_by(&self, block: CodeBlockId, function: FunctionCandidateId) -> bool {
        self.owners[block].contains(&function)
    }

    pub fn is_code(&self, block: CodeBlockId) -> bool {
        !self.owners[block].is_empty()
    }

    pub fn display<'a>(&'a self, inference: &'a InternalCallInference) -> OwnershipDisplay<'a> {
        OwnershipDisplay { ownership: self, inference }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OwnershipError {
    #[error("inferred function entry 0x{pc:x} is not a decoded JUMPDEST")]
    MissingFunctionEntry { pc: u32 },
}

pub struct OwnershipDisplay<'a> {
    ownership: &'a Ownership,
    inference: &'a InternalCallInference,
}

impl fmt::Display for OwnershipDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "functions:")?;
        for (id, function) in self.ownership.functions.enumerate_idx() {
            writeln!(f, "    f{id}: {:?} entry=@{}", function.kind, function.entry)?;
        }
        writeln!(f, "blocks:")?;
        for (block, code) in self.inference.code_blocks().blocks().enumerate_idx() {
            let owners = self.ownership.owners(block);
            match owners.len() {
                0 => writeln!(f, "    @{block} [0x{:x},0x{:x}) data", code.start_pc, code.end_pc)?,
                1 => {
                    let owner = owners.first().expect("checked one owner");
                    writeln!(
                        f,
                        "    @{block} [0x{:x},0x{:x}) owner=f{owner}",
                        code.start_pc, code.end_pc
                    )?;
                }
                _ => {
                    write!(
                        f,
                        "    @{block} [0x{:x},0x{:x}) duplicated in",
                        code.start_pc, code.end_pc
                    )?;
                    for owner in owners {
                        write!(f, " f{owner}")?;
                    }
                    writeln!(f)?;
                }
            }
        }
        Ok(())
    }
}
