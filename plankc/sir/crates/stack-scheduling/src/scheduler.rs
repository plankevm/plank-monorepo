use crate::{
    BlockFinalization,
    depth_first_search::{self, SearchConfig, SearchResult},
    greedy_intra_op_scheduler::greedy_schedule_op,
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpSetMut},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack},
    treegraph::build_tree_graph,
};
use sir_data::StaticAllocId;
use smallvec::SmallVec;

const SCRATCH_OP_SET_INLINE_CAPACITY: usize = 512 / BitsetWord::BITS as usize;

pub fn schedule(
    finalization: BlockFinalization,
    next_alloc_id: StaticAllocId,
    shuffle: ShuffleConfig,
    search: SearchConfig,
    graph: &OpGraph,
) -> SearchResult {
    let trees = build_tree_graph(graph);
    let mut result =
        depth_first_search::schedule(finalization, next_alloc_id, shuffle, search, &trees.graph);
    result.ops = trees.expand_schedule(graph, &result.ops);
    result
}

pub fn greedy_schedule(
    ops_sink: impl FnMut(StackOps),
    finalization: BlockFinalization,
    next_alloc_id: StaticAllocId,
    config: ShuffleConfig,
    graph: &OpGraph,
) -> StaticAllocId {
    let mut completable_backing = SmallVec::<[BitsetWord; SCRATCH_OP_SET_INLINE_CAPACITY]>::new();
    completable_backing.resize(graph.words_per_set() as usize, 0);
    let mut completable = OpSetMut::new(&mut completable_backing, graph.total_ops());

    let mut complete_backing = SmallVec::<[BitsetWord; SCRATCH_OP_SET_INLINE_CAPACITY]>::new();
    complete_backing.resize(graph.words_per_set() as usize, 0);
    let mut complete = OpSetMut::new(&mut complete_backing, graph.total_ops());

    let mut stack = {
        let mut inner = EvmStack::new();
        for input in graph.input_values_fifo().iter().rev() {
            inner.push(input);
        }
        TrackedStack::new_from_evm(next_alloc_id, ops_sink, inner, 8)
    };

    'schedule: loop {
        completable.clear();
        graph.collect_next_completable_into(complete.as_ref(), &mut completable);
        let Some(op) = completable.iter().next() else {
            break 'schedule;
        };
        greedy_schedule_op(config, &mut stack, graph, op, complete.as_ref(), false);
        complete.add(op);
    }

    if finalization == BlockFinalization::ShuffleToOutputs {
        greedy_shuffler::shuffle(config, &mut stack, graph);
    }

    stack.into_next_alloc_id()
}
