use plank_core::{DenseIndexMap, list_of_lists::ListOfLists, newtype_index};
use sir_data::{BasicBlockId, EthIRProgram};
use sir_passes::{AnalysesStore, ControlFlowGraphInOutBundling};

use layouts::{LayoutsTracker, build_basic_block_layout_sets};
use op_graph::build_graph_simple;
pub use stack::ScheduleConfig;
pub mod op_graph;

use crate::{scheduler::dumb_schedule, stack::StackOps};

mod greedy_shuffler;
mod layouts;
mod op_model;
mod scheduler;
pub mod stack;
mod state;

newtype_index! {
    pub struct StackOpIdx;
}

const AVG_OPS_PER_BLOCK: usize = 20;

#[derive(Debug)]
pub struct ScheduledOps {
    bb_to_ops: DenseIndexMap<BasicBlockId, StackOpIdx>,
    ops: ListOfLists<StackOpIdx, StackOps>,
}

impl ScheduledOps {
    pub fn get(&self, bb: BasicBlockId) -> Option<&[StackOps]> {
        self.bb_to_ops.get(bb).map(|&idx| &self.ops[idx])
    }

    pub fn enumerate_idx(&self) -> impl Iterator<Item = (BasicBlockId, &[StackOps])> {
        self.bb_to_ops.iter().map(|(bb_id, &idx)| (bb_id, &self.ops[idx]))
    }
}

pub fn schedule<'ir>(
    program: &'ir EthIRProgram,
    analyses: &AnalysesStore,
    config: ScheduleConfig,
) -> (ScheduledOps, LayoutsTracker<'ir>) {
    let in_out_bundling = ControlFlowGraphInOutBundling::new(program, analyses);
    let layout_sets = build_basic_block_layout_sets(program, analyses, &in_out_bundling);

    // Naively take layout sets as layouts since they are deterministically ordered.
    let layouts = LayoutsTracker::new(program, layout_sets, in_out_bundling);

    let mut bb_to_ops = DenseIndexMap::with_capacity(program.basic_blocks.len());
    let mut ops = ListOfLists::with_capacities(
        program.basic_blocks.len(),
        program.basic_blocks.len() * AVG_OPS_PER_BLOCK,
    );

    for block in program.blocks() {
        let Some((input_layout, output_layout)) = layouts.get_input_output(block.id()) else {
            continue;
        };

        let graph = build_graph_simple(program, block, &layouts, input_layout, output_layout);
        let ops_idx = ops.push_with(|mut pusher| {
            dumb_schedule(
                |op| pusher.push(op),
                block,
                &program.next_static_alloc_id,
                config,
                &graph,
            );
        });
        bb_to_ops.insert(block.id(), ops_idx);
    }

    (ScheduledOps { bb_to_ops, ops }, layouts)
}

#[cfg(test)]
mod tests;
