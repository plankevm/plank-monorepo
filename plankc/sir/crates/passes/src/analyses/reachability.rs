use crate::analyses::{AnalysesStore, cache::Analysis};
use plank_core::DenseIndexSet;
use sir_data::{BasicBlockId, EthIRProgram, FunctionId, Operation};

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

#[derive(Debug, Clone, Default)]
pub struct ReachableFunctions {
    functions: DenseIndexSet<FunctionId>,
    visited_blocks: DenseIndexSet<BasicBlockId>,
    worklist: Vec<BasicBlockId>,
}

impl Analysis for ReachableFunctions {
    fn compute(&mut self, program: &EthIRProgram, _store: &AnalysesStore) {
        self.functions.clear();
        self.visited_blocks.clear();
        assert!(self.worklist.is_empty());
        self.mark_reachable(program, program.init_entry);
        if let Some(main) = program.main_entry {
            self.mark_reachable(program, main);
        }
    }
}

impl ReachableFunctions {
    fn mark_reachable(&mut self, program: &EthIRProgram, function: FunctionId) {
        if !self.functions.add(function) {
            return;
        }

        self.worklist.push(program.functions[function].entry());
        while let Some(bb) = self.worklist.pop() {
            if !self.visited_blocks.add(bb) {
                continue;
            }

            for op in program.block(bb).operations() {
                if let Operation::InternalCall(data) = op.op() {
                    self.mark_reachable(program, data.function);
                }
            }

            for succ in program.block(bb).successors() {
                self.worklist.push(succ);
            }
        }
    }
}
