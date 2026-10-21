use crate::{
    BlockFinalization,
    greedy_intra_op_scheduler::greedy_schedule_op,
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpNodeKind, OpSetMut},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack},
    treegraph::build_tree_graph,
    validation,
};
use sir_data::StaticAllocId;
use smallvec::SmallVec;

const SCRATCH_OP_SET_INLINE_CAPACITY: usize = 512 / BitsetWord::BITS as usize;

pub struct SearchResult {
    pub ops: Box<[StackOps]>,
    pub spill_count: u32,
}

pub fn schedule(
    finalization: BlockFinalization,
    next_alloc_id: StaticAllocId,
    shuffle: ShuffleConfig,
    graph: &OpGraph,
) -> SearchResult {
    let trees = build_tree_graph(graph);

    let mut ops = Vec::new();
    let new_next_alloc_id =
        greedy_schedule(|op| ops.push(op), finalization, next_alloc_id, shuffle, &trees.graph);
    let spill_count = new_next_alloc_id - next_alloc_id;

    let ops = trees.expand_schedule(graph, &ops);

    validation::validate(graph, finalization, shuffle, next_alloc_id, new_next_alloc_id, &ops)
        .expect("stack scheduler produced an invalid schedule");

    SearchResult { ops, spill_count }
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
        // Cheapest-next-op selection is myopic: on the corpus it is 4-6% worse than
        // taking ready ops in graph order, which tracks the tree graph's operand order.
        let Some(op) = completable.iter().next() else {
            break 'schedule;
        };
        let flipped = matches!(graph.get_op(op).kind, OpNodeKind::Flippable(_)) && {
            let op_cost = |flipped| {
                let mut ops = Vec::new();
                let mut trial_stack = stack.clone_with(|op| ops.push(op));
                greedy_schedule_op(config, &mut trial_stack, graph, op, complete.as_ref(), flipped);
                crate::stack::gas_cost(&ops, config)
            };
            op_cost(true) < op_cost(false)
        };
        greedy_schedule_op(config, &mut stack, graph, op, complete.as_ref(), flipped);
        complete.add(op);
    }

    if finalization == BlockFinalization::ShuffleToOutputs {
        greedy_shuffler::shuffle(config, &mut stack, graph);
    }

    stack.into_next_alloc_id()
}
