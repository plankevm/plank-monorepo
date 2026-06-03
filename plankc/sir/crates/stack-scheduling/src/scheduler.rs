use crate::{
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpNodeId, OpSetMut, ValueNodeId},
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
    state::collect_next_completable_into,
};
use sir_data::{BlockView, ControlView, StaticAllocId};
use smallvec::SmallVec;
use std::cell::Cell;

// dumb intra-instruction scheduler that always dups its inputs.
fn dumb_schedule_op<Sink: FnMut(StackOps)>(
    config: ScheduleConfig,
    stack: &mut TrackedStack<'_, Sink>,
    graph: &OpGraph,
    op: OpNodeId,
    in_the_way_buf: &mut Vec<ValueNodeId>,
) {
    let max_dup_depth = u16::from(config.max_dup_depth);

    let op_view = graph.get_op(op);

    for &input in op_view.inputs_fifo.iter().rev() {
        let depth = stack.stack().find_first(input).expect("input missing");
        if depth <= max_dup_depth {
            stack.dup(depth as u8);
            continue;
        } else if let Some(spilled) = stack.get_spilled(input) {
            stack.load(spilled);
            continue;
        }

        let delta_to_max = depth - max_dup_depth;

        in_the_way_buf.clear();
        in_the_way_buf.extend_from_slice(&stack.fifo()[..delta_to_max as usize]);

        // Move minimum number of values out of the way.
        for _ in 0..delta_to_max {
            let top = stack.top().expect("no top despite beyond max depth");
            match stack.get_spilled(top) {
                Some(_) => stack.pop(),
                None => {
                    stack.spill_top();
                }
            }
        }

        // Now dup and spill.
        stack.dup(config.max_dup_depth);
        stack.spill_top();

        // Unspill in the way in correct order.
        for &spilled in in_the_way_buf.iter().rev() {
            stack.unspill(spilled);
        }

        // Load target value back
        stack.unspill(input);
    }
    stack.op(graph, op);
}

const SCRATCH_OP_SET_INLINE_CAPACITY: usize = 512 / BitsetWord::BITS as usize;

pub fn dumb_schedule(
    ops_sink: impl FnMut(StackOps),
    block: BlockView<'_>,
    next_alloc_id: &Cell<StaticAllocId>,
    config: ScheduleConfig,
    graph: &OpGraph,
) {
    let mut in_the_way_buf = Vec::with_capacity(4);

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
        collect_next_completable_into(graph, complete.as_ref(), &mut completable);
        let Some(op) = completable.iter().next() else {
            break 'schedule;
        };
        dumb_schedule_op(config, &mut stack, graph, op, &mut in_the_way_buf);
        complete.add(op);
    }

    if !matches!(block.control(), ControlView::LastOpTerminates) {
        greedy_shuffler::shuffle(config, &mut stack, graph);
    }
}
