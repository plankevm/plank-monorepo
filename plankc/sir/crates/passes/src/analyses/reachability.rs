use crate::analyses::{AnalysesStore, cache::Analysis};
use sir_data::{BasicBlockId, DenseIndexSet, EthIRProgram};

#[derive(Debug, Clone, Default)]
pub struct Reachability {
    reachable: DenseIndexSet<BasicBlockId>,
}

impl Analysis for Reachability {
    fn compute(&mut self, program: &EthIRProgram, _store: &AnalysesStore) {
        self.reachable.clear();

        for func in program.functions_iter() {
            self.mark_reachable(program, func.entry().id());
        }
    }
}

impl Reachability {
    fn mark_reachable(&mut self, program: &EthIRProgram, block: BasicBlockId) {
        if !self.reachable.add(block) {
            return;
        }

        for successor in program.block(block).successors() {
            self.mark_reachable(program, successor);
        }
    }

    pub fn contains(&self, block: BasicBlockId) -> bool {
        self.reachable.contains(block)
    }

    pub fn set_mut(&mut self) -> &mut DenseIndexSet<BasicBlockId> {
        &mut self.reachable
    }
}
