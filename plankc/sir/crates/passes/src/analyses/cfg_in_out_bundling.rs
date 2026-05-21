use plank_core::{DenseIndexMap, Idx, index, newtype_index, span::IncIterable};
use sir_data::{BasicBlockId, EthIRProgram};
use smallvec::SmallVec;

use crate::AnalysesStore;

newtype_index! {
    pub struct InOutGroupId;
}

#[derive(Debug, Default)]
pub struct ControlFlowGraphInOutBundling {
    out_group: DenseIndexMap<BasicBlockId, InOutGroupId>,
    in_group: DenseIndexMap<BasicBlockId, InOutGroupId>,
    next_group_id: InOutGroupId,
}

impl ControlFlowGraphInOutBundling {
    pub fn new(program: &EthIRProgram, analyses: &AnalysesStore) -> Self {
        let mut out_group = DenseIndexMap::with_capacity(program.basic_blocks.len());
        let mut in_group = DenseIndexMap::with_capacity(program.basic_blocks.len());
        let mut next_group_id = InOutGroupId::ZERO;

        let mut worklist = SmallVec::<[BasicBlockId; 64]>::new();
        let rpo = analyses.reverse_post_order(program);
        let preds = analyses.predecessors(program);

        for &bb_id in rpo.global_rpo() {
            worklist.push(bb_id);
            while let Some(bb_id) = worklist.pop() {
                let group_id = program
                    .block(bb_id)
                    .successors()
                    .find_map(|succ| in_group.get(succ).copied())
                    .unwrap_or_else(|| next_group_id.get_and_inc());
                out_group.insert(bb_id, group_id);
                for succ in program.block(bb_id).successors() {
                    let prev = in_group.insert(succ, group_id);
                    if prev.is_none_or(|existing_id| existing_id == group_id) {
                        continue;
                    }
                    for &pred in preds.of(succ) {
                        if pred != bb_id {
                            worklist.push(pred);
                        }
                    }
                }
            }
        }

        for function in program.functions.iter() {
            in_group.entry(function.entry()).or_insert_with(|| next_group_id.get_and_inc());
        }

        Self { out_group, in_group, next_group_id }
    }

    pub fn get_out_group(&self, bb_id: BasicBlockId) -> Option<InOutGroupId> {
        self.out_group.get(bb_id).copied()
    }

    pub fn get_in_group(&self, bb_id: BasicBlockId) -> Option<InOutGroupId> {
        self.in_group.get(bb_id).copied()
    }

    pub const fn total_groups(&self) -> u32 {
        self.next_group_id.const_get()
    }

    pub fn iter_groups(&self) -> impl Iterator<Item = InOutGroupId> {
        index::iter_until(self.next_group_id)
    }
}
