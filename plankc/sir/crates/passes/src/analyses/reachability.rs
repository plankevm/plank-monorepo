use crate::analyses::{AnalysesStore, cache::Analysis};
use plank_core::DenseIndexSet;
use sir_data::{BasicBlockId, EthIRProgram};

#[derive(Debug, Clone, Default)]
pub struct Reachability {
    reachable: DenseIndexSet<BasicBlockId>,
}

impl Analysis for Reachability {
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        self.reachable.clear();

        for &block in store.reverse_post_order(program).blocks_rpo() {
            self.reachable.add(block);
        }
    }
}

impl Reachability {
    pub fn contains(&self, block: BasicBlockId) -> bool {
        self.reachable.contains(block)
    }

    pub fn set_mut(&mut self) -> &mut DenseIndexSet<BasicBlockId> {
        &mut self.reachable
    }
}
