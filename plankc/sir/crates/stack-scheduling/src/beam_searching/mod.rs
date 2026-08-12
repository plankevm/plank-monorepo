use crate::{
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

pub fn searching_schedule(
    result_ops_sink: impl FnMut(StackOps),
    block: BlockView<'_>,
    mut next_alloc_id: StaticAllocId,
    shuffle: ShuffleConfig,
    schedule: ScheduleConfig,
    graph: &OpGraph,
) -> StaticAllocId {
    let mut beam =
        Vec::with_capacity(schedule.beam_width.max(1) * graph.total_ops().div_ceil(2) as usize);
    beam.push(ScheduleSearchState::start(graph));

    let mut completable_backing = SmallVec::<[BitsetWord; SCRATCH_OP_SET_INLINE_CAPACITY]>::new();
    completable_backing.resize(graph.words_per_set() as usize, 0);
    let mut completable = OpSetMut::new(&mut completable_backing, graph.total_ops());

    /* beam search loop */
    {
        completable.clear();
        let complete: &[BitsetWord] = todo!();
        graph.collect_next_completable_into(
            OpSet::new(complete, graph.total_ops()),
            &mut completable,
        );
    }

    next_alloc_id
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
