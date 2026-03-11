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

    pub fn uses_of(&self, local: LocalId) -> &[UseLocation] {
        &self.inner[local]
    }

    pub fn retain(&mut self, local: LocalId, f: impl FnMut(&UseLocation) -> bool) {
        self.inner[local].retain(f);
    }
}
