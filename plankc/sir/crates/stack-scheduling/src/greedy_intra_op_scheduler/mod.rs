use crate::{
    greedy_intra_op_scheduler::state::{Candidate, IntraOp, StateKey, TargetCtx},
    op_graph::{OpGraph, OpNodeId, OpSet},
    stack::{ScheduleConfig, StackOps, TrackedStack},
};
use hashbrown::{HashMap, hash_map::Entry};
use plank_core::LoopLimit;

mod state;

#[cfg(test)]
mod tests;

pub(crate) fn greedy_schedule_op<Sink: FnMut(StackOps)>(
    config: ScheduleConfig,
    result_stack: &mut TrackedStack<Sink>,
    graph: &OpGraph,
    op_id: OpNodeId,
    complete: OpSet<'_>,
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

    let mut state_to_lowest_cost = HashMap::<StateKey, u32>::new();
    let mut current = vec![Candidate {
        stack: result_stack.fifo().into(),
        spilled: result_stack.underlying_spilled().to_vec(),
        cost_so_far: 0,
        todo: todo_count.try_into().expect("overflow u32"),
        estimated_remaining_cost: 0,
        ops: Vec::new(),
    }];
    let mut next = vec![];

    let target = TargetCtx { config, target: op.inputs_fifo, last_uses: &unique_last_uses };

    let mut limit = LoopLimit::max(100_000);
    let solution = loop {
        limit.tick();

        current.drain(..).for_each(|candidate| {
            candidate.expand(target, |new| match state_to_lowest_cost.entry(new.key()) {
                Entry::Occupied(mut existing) => {
                    let existing = existing.get_mut();
                    if *existing > new.cost_so_far {
                        *existing = new.cost_so_far;
                        next.push(new);
                    }
                }
                Entry::Vacant(vacant) => {
                    vacant.insert_entry(new.cost_so_far);
                    next.push(new);
                }
            });
        });
        next.sort_by_key(|c| c.cost_so_far + c.estimated_remaining_cost);
        next.truncate(beam_width.into());

        let best = next.first().expect("beam empty");
        if best.complete() {
            break best;
        }

        std::mem::swap(&mut current, &mut next);
    };

    for &op in &solution.ops {
        match op {
            IntraOp::Swap(depth) => result_stack.swap(depth),
            IntraOp::Dup(depth) => result_stack.dup(depth),
            IntraOp::SpillTop => {
                result_stack.spill_top();
            }
            IntraOp::Unspill(value) => result_stack.unspill(value),
        }
    }

    result_stack.op(graph, op_id, false);
}
