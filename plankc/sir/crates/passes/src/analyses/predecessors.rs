use crate::analyses::{AnalysesStore, cache::Analysis};
use sir_data::{BasicBlockId, EthIRProgram, IndexVec};

#[derive(Default)]
pub struct Predecessors {
    inner: IndexVec<BasicBlockId, Vec<BasicBlockId>>,
}

impl Analysis for Predecessors {
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        let reachability = store.reachability(program);

        for pred in self.inner.iter_mut() {
            pred.clear();
        }
        self.inner.resize(program.basic_blocks.len(), Vec::new());

        for block in program.blocks() {
            if !reachability.contains(block.id()) {
                continue;
            }
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

    pub fn enumerate(&self) -> impl Iterator<Item = (BasicBlockId, &[BasicBlockId])> {
        self.inner.enumerate_idx().map(|(bb, preds)| (bb, preds.as_slice()))
    }
}
