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
    beam_width: usize,
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
    let beam_capacity = schedule.beam_width.max(1) * graph.total_ops().div_ceil(2) as usize;
    let mut beam = Vec::with_capacity(beam_capacity);
    beam.push(ScheduleSearchState::start(graph));
    let mut next_beam = Vec::with_capacity(beam_capacity);

    let mut completable_backing = SmallVec::<[BitsetWord; SCRATCH_OP_SET_INLINE_CAPACITY]>::new();
    completable_backing.resize(graph.words_per_set() as usize, 0);
    let mut completable = OpSetMut::new(&mut completable_backing, graph.total_ops());

    for _ in 0..graph.total_ops() {
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
        next_beam.truncate(schedule.beam_width);

        std::mem::swap(&mut beam, &mut next_beam);
    }

    let best = beam.first().expect("beam empty");

    for &op in &best.executed {
        result_ops_sink(op);
    }

    if !matches!(block.control(), ControlView::LastOpTerminates) {
        let mut stack = TrackedStack::new_from_parts(
            next_alloc_id,
            result_ops_sink,
            best.stack_fifo(),
            best.spilled().to_vec(),
        );
        greedy_shuffler::shuffle(shuffle, &mut stack, graph);
        return stack.into_next_alloc_id();
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
