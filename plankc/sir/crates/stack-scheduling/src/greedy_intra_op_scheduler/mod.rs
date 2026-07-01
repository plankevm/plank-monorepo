use crate::{
    greedy_intra_op_scheduler::state::Candidate,
    op_graph::{OpGraph, OpNodeId, OpNodeKind, OpSet, ValueNodeId},
    stack::{ScheduleConfig, StackOps, TrackedStack},
};
use hashbrown::HashMap;
use plank_core::LoopLimit;

mod actions;
mod state;

#[cfg(test)]
mod tests;

pub(crate) fn greedy_schedule_op<Sink: FnMut(StackOps)>(
    config: ScheduleConfig,
    result_stack: &mut TrackedStack<Sink>,
    graph: &OpGraph,
    op_id: OpNodeId,
    complete: OpSet<'_>,
    beam_width: u16,
) {
    let op = graph.get_op(op_id);

    let mut unique_last_uses = Vec::with_capacity(op.inputs_fifo.len() / 2);
    for &value in op.inputs_fifo {
        if graph.is_last_use(complete, value) && !unique_last_uses.contains(&value) {
            unique_last_uses.push(value);
        }
    }
    let working_window_start_size = unique_last_uses.len();
    let target_depth_delta = op.inputs_fifo.len() - working_window_start_size;
    let mut todo_count = target_depth_delta;
    for (current, target) in result_stack.fifo().iter().zip(&op.inputs_fifo[target_depth_delta..]) {
        if current != target {
            todo_count += 1;
            if !op.inputs_fifo.contains(current) {
                todo_count += 1;
            }
        }
    }

    let mut best = HashMap::<Box<[ValueNodeId]>, u32>::new();
    let mut current = vec![Candidate {
        stack: result_stack.fifo().into(),
        cost_so_far: 0,
        todo: todo_count.try_into().expect("overflow u32"),
        ops: Vec::new(),
    }];
    let mut next = vec![];

    let mut limit = LoopLimit::max(100_000);
    let solution = loop {
        limit.tick();

        for candidate in current.drain(..) {}

        next.truncate(beam_width.into());
        std::mem::swap(&mut current, &mut next);
    };

    result_stack.op(graph, op_id, false);
}
