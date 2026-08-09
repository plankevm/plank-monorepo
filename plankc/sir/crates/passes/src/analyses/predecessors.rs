use crate::analyses::{AnalysesStore, cache::Analysis};
use plank_core::IndexVec;
use sir_data::{BasicBlockId, EthIRProgram};

#[derive(Default)]
pub struct Predecessors {
    inner: IndexVec<BasicBlockId, Vec<BasicBlockId>>,
}

impl Analysis for Predecessors {
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        let reachable_blocks = store.reachable_blocks(program);

        for pred in self.inner.iter_mut() {
            pred.clear();
        }
        self.inner.resize(program.basic_blocks.len(), Vec::new());

        for block in program.blocks() {
            if !reachable_blocks.contains(block.id()) {
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

    /// Replaces one incoming edge. Call once per edge when the predecessor occurs multiple times.
    pub fn replace_predecessor_edge(
        &mut self,
        bb: BasicBlockId,
        old: BasicBlockId,
        new: BasicBlockId,
    ) {
        let predecessor = self.inner[bb]
            .iter_mut()
            .find(|predecessor| **predecessor == old)
            .expect("old predecessor should exist");
        *predecessor = new;
    }

    pub fn clear_predecessors(&mut self, bb: BasicBlockId) {
        self.inner[bb].clear();
    }

    pub fn enumerate(&self) -> impl Iterator<Item = (BasicBlockId, &[BasicBlockId])> {
        self.inner.enumerate_idx().map(|(bb, preds)| (bb, preds.as_slice()))
    }
}
