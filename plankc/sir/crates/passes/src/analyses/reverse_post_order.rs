use crate::analyses::{AnalysesStore, cache::Analysis, dfs_postorder};
use sir_data::{BasicBlockId, DenseIndexSet, EthIRProgram};

#[derive(Debug, Clone, Default)]
pub struct ReversePostOrder {
    visited: DenseIndexSet<BasicBlockId>,
    order: Vec<BasicBlockId>,
}

impl Analysis for ReversePostOrder {
    fn compute(&mut self, program: &EthIRProgram, _store: &AnalysesStore) {
        self.order.clear();
        self.visited.clear();

        for func in program.functions_iter() {
            dfs_postorder(program, func.entry().id(), &mut self.visited, &mut self.order);
        }

        self.order.reverse();
    }
}

impl ReversePostOrder {
    pub fn order(&self) -> &[BasicBlockId] {
        &self.order
    }
}
