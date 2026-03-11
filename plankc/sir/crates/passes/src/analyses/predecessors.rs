use crate::analyses::{AnalysesStore, cache::Analysis};
use sir_data::{BasicBlockId, EthIRProgram, IndexVec};

#[derive(Default)]
pub struct Predecessors {
    pub(crate) inner: IndexVec<BasicBlockId, Vec<BasicBlockId>>,
}

impl Analysis for Predecessors {
    fn compute(&mut self, program: &EthIRProgram, _store: &AnalysesStore) {
        for pred in self.inner.iter_mut() {
            pred.clear();
        }
        self.inner.resize(program.basic_blocks.len(), Vec::new());

        for block in program.blocks() {
            for successor in block.successors() {
                self.inner[successor].push(block.id());
            }
        }
    }
}

impl Predecessors {
    pub fn of(&self, bb: BasicBlockId) -> &[BasicBlockId] {
        &self.inner[bb]
    }
}
