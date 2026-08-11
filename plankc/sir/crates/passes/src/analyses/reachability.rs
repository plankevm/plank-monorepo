use crate::analyses::{AnalysesStore, cache::Analysis};
use plank_core::DenseIndexSet;
use sir_data::{BasicBlockId, EthIRProgram};

#[derive(Debug, Clone, Default)]
pub struct ReachableBlocks {
    blocks: DenseIndexSet<BasicBlockId>,
}

impl Analysis for ReachableBlocks {
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        self.blocks.clear();

        for &block in store.reverse_post_order(program).blocks_rpo() {
            self.blocks.add(block);
        }
    }
}

impl ReachableBlocks {
    pub fn contains(&self, block: BasicBlockId) -> bool {
        self.blocks.contains(block)
    }

    pub fn set_mut(&mut self) -> &mut DenseIndexSet<BasicBlockId> {
        &mut self.blocks
    }
}
