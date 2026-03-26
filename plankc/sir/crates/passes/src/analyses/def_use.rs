use crate::analyses::{AnalysesStore, cache::Analysis};
use sir_data::{BasicBlockId, ControlView, EthIRProgram, Idx, IndexVec, LocalId, OperationIdx};

#[derive(Clone)]
pub struct UseLocation {
    pub block_id: BasicBlockId,
    pub kind: UseKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseKind {
    Operation(OperationIdx),
    Control,
    BlockOutput(u32),
}

impl std::fmt::Display for UseKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UseKind::Operation(op) => write!(f, "operation {op}"),
            UseKind::Control => write!(f, "control"),
            UseKind::BlockOutput(idx) => write!(f, "block output {idx}"),
        }
    }
}

#[derive(Default)]
pub struct DefUse {
    uses: IndexVec<LocalId, Vec<UseLocation>>,
}

impl Analysis for DefUse {
    fn compute(&mut self, program: &EthIRProgram, _store: &AnalysesStore) {
        let num_locals = program.next_free_local_id.idx();
        for vec in self.uses.iter_mut() {
            vec.clear();
        }
        self.uses.resize_with(num_locals, Vec::new);

        for block in program.blocks() {
            for op in block.operations() {
                for &input in op.inputs() {
                    self.uses[input].push(UseLocation {
                        block_id: block.id(),
                        kind: UseKind::Operation(op.id()),
                    });
                }
            }

            match block.control() {
                ControlView::Branches { condition, .. } => {
                    self.uses[condition]
                        .push(UseLocation { block_id: block.id(), kind: UseKind::Control });
                }
                ControlView::Switch(switch) => {
                    self.uses[switch.condition()]
                        .push(UseLocation { block_id: block.id(), kind: UseKind::Control });
                }
                _ => {}
            }

            for (idx, &local) in block.outputs().iter().enumerate() {
                self.uses[local].push(UseLocation {
                    block_id: block.id(),
                    kind: UseKind::BlockOutput(idx as u32),
                });
            }
        }
    }
}

impl DefUse {
    pub fn uses_of(&self, local: LocalId) -> &[UseLocation] {
        &self.uses[local]
    }

    pub fn retain(&mut self, local: LocalId, f: impl FnMut(&UseLocation) -> bool) {
        self.uses[local].retain(f);
    }
}
