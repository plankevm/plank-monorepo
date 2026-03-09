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
    BlockOutput,
}

impl std::fmt::Display for UseKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UseKind::Operation(op) => write!(f, "operation {op}"),
            UseKind::Control => write!(f, "control"),
            UseKind::BlockOutput => write!(f, "block output"),
        }
    }
}

#[derive(Default)]
pub struct DefUse {
    inner: IndexVec<LocalId, Vec<UseLocation>>,
}

impl std::ops::Index<LocalId> for DefUse {
    type Output = Vec<UseLocation>;
    fn index(&self, id: LocalId) -> &Vec<UseLocation> {
        &self.inner[id]
    }
}

impl std::ops::IndexMut<LocalId> for DefUse {
    fn index_mut(&mut self, id: LocalId) -> &mut Vec<UseLocation> {
        &mut self.inner[id]
    }
}

impl DefUse {
    pub fn compute(&mut self, program: &EthIRProgram) {
        let num_locals = program.next_free_local_id.idx();
        for vec in self.inner.iter_mut() {
            vec.clear();
        }
        self.inner.resize_with(num_locals, Vec::new);

        for block in program.blocks() {
            for op in block.operations() {
                for &input in op.inputs() {
                    self.inner[input].push(UseLocation {
                        block_id: block.id(),
                        kind: UseKind::Operation(op.id()),
                    });
                }
            }

            match block.control() {
                ControlView::Branches { condition, .. } => {
                    self.inner[condition]
                        .push(UseLocation { block_id: block.id(), kind: UseKind::Control });
                }
                ControlView::Switch(switch) => {
                    self.inner[switch.condition()]
                        .push(UseLocation { block_id: block.id(), kind: UseKind::Control });
                }
                _ => {}
            }

            for &local in block.outputs() {
                self.inner[local]
                    .push(UseLocation { block_id: block.id(), kind: UseKind::BlockOutput });
            }
        }
    }
}
