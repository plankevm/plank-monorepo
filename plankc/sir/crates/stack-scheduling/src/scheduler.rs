#![allow(unused)]

use crate::{
    greedy_intra_op_scheduler::greedy_schedule_op,
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpSetMut},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack},
};
use sir_data::{BlockView, ControlView, StaticAllocId};
use smallvec::SmallVec;

const SCRATCH_OP_SET_INLINE_CAPACITY: usize = 512 / BitsetWord::BITS as usize;

pub fn greedy_schedule(
    ops_sink: impl FnMut(StackOps),
    block: BlockView<'_>,
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
        greedy_schedule_op(config, &mut stack, graph, op, complete.as_ref());
        complete.add(op);
    }

    if !matches!(block.control(), ControlView::LastOpTerminates) {
        greedy_shuffler::shuffle(config, &mut stack, graph);
    }

    stack.into_next_alloc_id()
}
