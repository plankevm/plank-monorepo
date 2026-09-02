use std::num::NonZero;

use plank_core::{DenseIndexMap, list_of_lists::ListOfLists, newtype_index};
use rayon::prelude::*;
use sir_data::{BasicBlockId, EthIRProgram, StaticAllocId};
use sir_passes::{AnalysesStore, ControlFlowGraphInOutBundling};
#[cfg(test)]
use {clap as _, csv as _};

use layouts::{LayoutsTracker, build_basic_block_layout_sets};
pub use stack::ShuffleConfig;
pub mod op_graph;

use crate::{op_graph::build_graph_effectful, stack::StackOps};

mod depth_first_search;
mod greedy_intra_op_scheduler;
mod greedy_shuffler;
mod layouts;
mod scheduler;
pub mod stack;
pub mod treegraph;

newtype_index! {
    pub struct StackOpIdx;
}

const AVG_OPS_PER_BLOCK: usize = 20;
const DEFAULT_MAX_SEARCH_CANDIDATES: usize = 1_000;
const BLOCK_SCHEDULING_THREADS: usize = 6;

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
    config: ShuffleConfig,
) -> (ScheduledOps, LayoutsTracker<'ir>, StaticAllocId) {
    let in_out_bundling = ControlFlowGraphInOutBundling::new(program, analyses);
    let layout_sets = build_basic_block_layout_sets(program, analyses, &in_out_bundling);
    let mut next_alloc_id = program.next_static_alloc_id;

    // Naively take layout sets as layouts since they are deterministically ordered.
    let layouts = LayoutsTracker::new(program, layout_sets, in_out_bundling);

    let mut bb_to_ops = DenseIndexMap::with_capacity(program.basic_blocks.len());
    let mut ops = ListOfLists::with_capacities(
        program.basic_blocks.len(),
        program.basic_blocks.len() * AVG_OPS_PER_BLOCK,
    );

    let block_graphs = program
        .blocks()
        .filter_map(|block| {
            let (input_layout, output_layout) = layouts.get_input_output(block.id())?;
            let graph = build_graph_effectful(
                program,
                block,
                &layouts,
                input_layout,
                output_layout,
                analyses,
            );
            Some((block, graph))
        })
        .collect::<Vec<_>>();
    // Blocks share a temporary spill base while scheduling so they can run independently. Their
    // block-local spill IDs are rebased after the parallel search finishes.
    let local_alloc_start = next_alloc_id;
    let scheduling_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(BLOCK_SCHEDULING_THREADS)
        .build()
        .expect("failed to create block scheduling thread pool");
    let block_schedules = scheduling_pool.install(|| {
        block_graphs
            .into_par_iter()
            .map(|(block, graph)| {
                let result = depth_first_search::schedule(
                    block,
                    local_alloc_start,
                    config,
                    depth_first_search::SearchConfig {
                        max_candidates: NonZero::new(DEFAULT_MAX_SEARCH_CANDIDATES).unwrap(),
                    },
                    &graph,
                );
                (block.id(), result)
            })
            .collect::<Vec<_>>()
    });

    for (block_id, schedule) in block_schedules {
        let alloc_offset = next_alloc_id - local_alloc_start;
        let ops_idx = ops.push_iter(schedule.ops.into_iter().map(|op| match op {
            StackOps::Store(id) => StackOps::Store(id + alloc_offset),
            StackOps::Load(id) => StackOps::Load(id + alloc_offset),
            op => op,
        }));
        next_alloc_id += schedule.spill_count;
        bb_to_ops.insert(block_id, ops_idx);
    }

    (ScheduledOps { bb_to_ops, ops }, layouts, next_alloc_id)
}

#[cfg(test)]
mod tests;
