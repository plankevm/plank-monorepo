use std::num::NonZero;

use crate::{
    greedy_intra_op_scheduler::greedy_schedule_op,
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpSet, OpSetMut},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack},
};
use sir_data::{BlockView, ControlView, StaticAllocId};
use smallvec::SmallVec;
use state::ScheduleSearchState;

mod state;

pub struct ScheduleConfig {
    beam_width: NonZero<usize>,
}

const SCRATCH_OP_SET_INLINE_CAPACITY: usize = 512 / BitsetWord::BITS as usize;
const ESTIMATED_STACK_OPS_PER_GRAPH_OP: usize = 8;

pub fn searching_schedule(
    mut result_ops_sink: impl FnMut(StackOps),
    block: BlockView<'_>,
    next_alloc_id: StaticAllocId,
    shuffle: ShuffleConfig,
    schedule: ScheduleConfig,
    graph: &OpGraph,
) -> StaticAllocId {
    if graph.total_ops() == 0 {
        let mut inner = EvmStack::new();
        for input in graph.input_values_fifo().iter().rev() {
            inner.push(input);
        }
        let mut stack = TrackedStack::new_from_evm(next_alloc_id, result_ops_sink, inner, 8);

        if !matches!(block.control(), ControlView::LastOpTerminates) {
            greedy_shuffler::shuffle(shuffle, &mut stack, graph);
        }

        return stack.into_next_alloc_id();
    }

    let beam_capacity = schedule.beam_width.get() * graph.total_ops().div_ceil(2) as usize;
    let mut beam = Vec::with_capacity(beam_capacity);
    beam.push(ScheduleSearchState::start(graph));
    let mut next_beam = Vec::with_capacity(beam_capacity);

    let mut completable_backing = SmallVec::<[BitsetWord; SCRATCH_OP_SET_INLINE_CAPACITY]>::new();
    completable_backing.resize(graph.words_per_set() as usize, 0);
    let mut completable = OpSetMut::new(&mut completable_backing, graph.total_ops());

    for op_idx in 0..graph.total_ops() {
        let is_last = op_idx == graph.total_ops() - 1;

        for state in beam.drain(..) {
            completable.clear();
            let complete = state.complete(graph.total_ops());
            graph.collect_next_completable_into(complete, &mut completable);

            let mut new_executed =
                Vec::with_capacity(state.executed.len() + ESTIMATED_STACK_OPS_PER_GRAPH_OP);
            new_executed.extend_from_slice(&state.executed);

            for op in completable.iter() {
                new_executed.truncate(state.executed.len());

                let mut stack = TrackedStack::new_from_parts(
                    next_alloc_id,
                    |op| new_executed.push(op),
                    state.stack_fifo(),
                    state.spilled().to_vec(),
                );

                greedy_schedule_op(shuffle, &mut stack, graph, op, complete);

                if is_last && !matches!(block.control(), ControlView::LastOpTerminates) {
                    greedy_shuffler::shuffle(shuffle, &mut stack, graph);
                }

                let values = [stack.fifo(), stack.underlying_spilled()].concat();
                let stack_end = stack.fifo().len();
                let new_cost = new_executed[state.executed.len()..]
                    .iter()
                    .map(|&op| u32::from(op_cost(op, shuffle)))
                    .sum::<u32>();
                next_beam.push(ScheduleSearchState {
                    complete: {
                        let mut backing = complete.clone_backing();
                        OpSetMut::new(&mut backing, graph.total_ops()).add(op);
                        backing.into_boxed_slice()
                    },
                    executed: new_executed.as_slice().into(),
                    executed_cost: state.executed_cost + new_cost,
                    values: values.into_boxed_slice(),
                    stack_end,
                });
            }
        }

        next_beam.sort_unstable_by_key(|beam| beam.executed_cost);

        let max_candidate_count = if is_last { 1 } else { schedule.beam_width.get() };
        let mut deduplicated_len = 0;

        for candidate_idx in 0..next_beam.len() {
            if deduplicated_len == max_candidate_count {
                break;
            }

            let is_redundant = next_beam[..deduplicated_len]
                .iter()
                .any(|retained| retained.is_redundant(&next_beam[candidate_idx]));
            if is_redundant {
                continue;
            }

            next_beam.swap(deduplicated_len, candidate_idx);
            deduplicated_len += 1;
        }
        next_beam.truncate(deduplicated_len);

        std::mem::swap(&mut beam, &mut next_beam);
    }

    let [best] = beam.as_slice() else {
        unreachable!("last loop should leave just the one best candidate")
    };

    for &op in &best.executed {
        result_ops_sink(op);
    }

    next_alloc_id + u32::try_from(best.spilled().len()).expect("overflow")
}

fn op_cost(op: StackOps, config: ShuffleConfig) -> u8 {
    match op {
        // These represent necessary basic block operations and therefore shouldn't be
        StackOps::Flipped(_) | StackOps::Op(_) | StackOps::CallRetPush(_) => 0,
        StackOps::Swap(_) | StackOps::Dup(_) | StackOps::Pop => 3,
        StackOps::Exchange(_, _) => config.exchange_cost,
        // Conservatively assume store will need to pay for memory expansion
        StackOps::Store(_) => 9,
        StackOps::Load(_) => 6,
    }
}
