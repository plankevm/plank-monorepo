use crate::{
    BlockFinalization,
    greedy_intra_op_scheduler::greedy_schedule_op,
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpNodeId, OpNodeKind, OpSetMut},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack},
    treegraph::build_tree_graph,
    validation,
};
use plank_core::DenseIndexSet;
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

fn output_priority(graph: &OpGraph, operation: OpNodeId) -> usize {
    let mut pending = graph.get_op(operation).outputs_fifo.to_vec();
    let mut seen = DenseIndexSet::with_capacity_in_bits(graph.total_values() as usize);
    let mut best = 0;
    while let Some(value) = pending.pop() {
        if !seen.add(value) {
            continue;
        }
        if let Some(position) =
            graph.output_values_fifo().iter().position(|&output| output == value)
        {
            best = best.max(position + 1);
        }
        for consumer in graph.get_consumers(value).iter() {
            pending.extend_from_slice(graph.get_op(consumer).outputs_fifo);
        }
    }
    best
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
        let mut best = None;
        for op in completable.iter() {
            let mut normal_ops = Vec::new();
            let mut normal_stack = stack.clone_with(|op| normal_ops.push(op));
            greedy_schedule_op(config, &mut normal_stack, graph, op, complete.as_ref(), false);
            let normal_cost = crate::stack::gas_cost(&normal_ops, config);

            let (cost, flipped) = if matches!(graph.get_op(op).kind, OpNodeKind::Flippable(_)) {
                let mut flipped_ops = Vec::new();
                let mut flipped_stack = stack.clone_with(|op| flipped_ops.push(op));
                greedy_schedule_op(config, &mut flipped_stack, graph, op, complete.as_ref(), true);
                let flipped_cost = crate::stack::gas_cost(&flipped_ops, config);
                if flipped_cost < normal_cost { (flipped_cost, true) } else { (normal_cost, false) }
            } else {
                (normal_cost, false)
            };
            let output_priority = output_priority(graph, op);
            let better = best.is_none_or(|(best_cost, _, _, _)| cost > best_cost);
            if better {
                best = Some((cost, op, flipped, output_priority));
            }
        }
        let Some((_, op, flipped, _)) = best else {
            break 'schedule;
        };
        greedy_schedule_op(config, &mut stack, graph, op, complete.as_ref(), flipped);
        complete.add(op);
    }

    if finalization == BlockFinalization::ShuffleToOutputs {
        greedy_shuffler::shuffle(config, &mut stack, graph);
    }

    stack.into_next_alloc_id()
}
