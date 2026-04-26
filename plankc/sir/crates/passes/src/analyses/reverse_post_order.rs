use crate::analyses::{AnalysesStore, cache::Analysis};
use sir_data::{BasicBlockId, DenseIndexSet, EthIRProgram, FunctionId, IndexVec};

#[derive(Debug, Clone, Default)]
pub struct ReversePostOrder {
    visited: DenseIndexSet<BasicBlockId>,
    global_rpo: Vec<BasicBlockId>,
    function_to_start_pre_rev: IndexVec<FunctionId, u32>,
}

impl Analysis for ReversePostOrder {
    fn compute(&mut self, program: &EthIRProgram, _store: &AnalysesStore) {
        fn dfs_postorder(
            program: &EthIRProgram,
            entry: BasicBlockId,
            visited: &mut DenseIndexSet<BasicBlockId>,
            postorder: &mut Vec<BasicBlockId>,
        ) {
            if !visited.add(entry) {
                return;
            }

            for succ in program.block(entry).successors() {
                dfs_postorder(program, succ, visited, postorder);
            }
            postorder.push(entry);
        }

        self.global_rpo.clear();
        self.visited.clear();
        self.function_to_start_pre_rev.clear();

        self.global_rpo.reserve_exact(program.basic_blocks.len());
        self.function_to_start_pre_rev.reserve_exact(program.functions.len());

        for func in program.functions_iter() {
            let start = self.global_rpo.len() as u32;
            let id = self.function_to_start_pre_rev.push(start);
            assert_eq!(id, func.id());
            dfs_postorder(program, func.entry().id(), &mut self.visited, &mut self.global_rpo);
        }
        self.global_rpo.reverse();
    }
}

impl ReversePostOrder {
    pub fn global_rpo(&self) -> &[BasicBlockId] {
        &self.global_rpo
    }

    pub fn function_rpo(&self, func: FunctionId) -> &[BasicBlockId] {
        // The offsets in `function_to_start_pre_rev` are computed relative to the `global_rpo`
        // *before* it's `.reverse()`'d so we need to compute the reverse indices and flip
        // start/end.
        let start = self
            .function_to_start_pre_rev
            .get(func + 1)
            .map_or(0, |&start| self.global_rpo.len() - start as usize);
        let end = self.global_rpo.len() - self.function_to_start_pre_rev[func] as usize;
        &self.global_rpo[start..end]
    }

    pub fn global_post_order(&self) -> impl Iterator<Item = BasicBlockId> {
        self.global_rpo.iter().rev().copied()
    }
}
