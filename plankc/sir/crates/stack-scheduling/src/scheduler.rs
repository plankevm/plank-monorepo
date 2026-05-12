use std::cell::Cell;

use plank_core::list_of_lists::ListOfListsPusher;
use sir_data::{BasicBlockId, IndexVec, StaticAllocId, index_vec};

use crate::{
    op_graph::*,
    stack::{ScheduleConfig, StackOps, TrackedStack, VirtualStack},
};

fn get_operation_topological_sort(graph: &OpGraph) -> Vec<OpNodeId> {
    let mut in_degrees: IndexVec<OpNodeId, u32> = index_vec![0; graph.operations.len()];
    for (id, op) in graph.operations.enumerate_idx() {
        in_degrees[id] +=
            op.consumes_fifo.iter().filter(|&&value| !graph.is_input(value)).count() as u32;
        for &succ in &op.happens_before {
            in_degrees[succ] += 1;
        }
    }

    let mut topo_sort = Vec::with_capacity(graph.operations.len());
    let mut queue = Vec::with_capacity(128);

    for (id, &in_degree) in in_degrees.enumerate_idx() {
        if in_degree == 0 {
            queue.push(id);
        }
    }

    while let Some(id) = queue.pop() {
        topo_sort.push(id);
        let next = &graph.operations[id];
        for &succ in next
            .produces_fifo
            .iter()
            .flat_map(|&output| &graph.values[output].used_by)
            .chain(&next.happens_before)
        {
            in_degrees[succ] -= 1;
            if in_degrees[succ] == 0 {
                queue.push(succ);
            }
        }
    }

    topo_sort
}

// dumb intra-instruction scheduler that always dups its inputs.
fn schedule_op(config: ScheduleConfig, stack: &mut TrackedStack, graph: &OpGraph, op: OpNodeId) {
    let max_dup_depth = u16::from(config.max_dup_depth);
    for &input in graph.operations[op].consumes_fifo.iter().rev() {
        let depth = stack.stack().find_first(input).expect("input missing");
        if depth <= max_dup_depth {
            stack.dup(depth as u8);
            continue;
        }

        let delta_to_max = depth - max_dup_depth;

        // Move minimum number of values out of the way.
        for spill_slot in 0..delta_to_max {
            stack.store(spill_slot.into());
        }

        // Now dup and spill.
        stack.dup(config.max_dup_depth);
        stack.store(delta_to_max.into());

        // Unspill in the way in correct order.
        for spill_slot in (0..delta_to_max).rev() {
            stack.load(spill_slot.into());
        }

        // Load target value back
        stack.load(delta_to_max.into());
    }
    stack.op(graph, op);
}

fn count_occurences(values: &[ValueNodeId], total_values: usize) -> IndexVec<ValueNodeId, u16> {
    let mut counts = IndexVec::new();
    counts.resize(total_values, 0);
    for &value in values {
        counts[value] += 1;
    }
    counts
}

fn shuffle_to_output(_config: ScheduleConfig, stack: &mut TrackedStack, graph: &OpGraph) {
    let target_stack = graph.end_stack_fifo.as_slice();
    let target_counts = count_occurences(target_stack, graph.values.len());

    for _ in 0..stack.len() {
        let top = stack.top().expect("shouldn't pop more than one per loop");
        if target_counts[top] == 0 {
            stack.pop();
            continue;
        }
        // Already spilled.
        if stack.spilled().any(|(_slot, value)| value == top) {
            stack.pop();
            continue;
        }
        stack.store('next_free_slot: {
            let mut free_slot = 0u32;
            for (slot, _) in stack.spilled() {
                if free_slot < slot {
                    break 'next_free_slot free_slot;
                }
                free_slot = slot + 1;
            }
            free_slot
        });
    }

    for &target in target_stack.iter().rev() {
        let slot = stack
            .spilled()
            .find_map(|(slot, value)| (value == target).then_some(slot))
            .expect("missing value in spilled");
        stack.load(slot);
    }
}

pub fn dumb_schedule<'p, 'a: 'p>(
    ops_sink: &'p mut ListOfListsPusher<'a, BasicBlockId, StackOps>,
    next_alloc_id: &'a Cell<StaticAllocId>,
    config: ScheduleConfig,
    graph: &'a OpGraph,
) {
    let mut stack = VirtualStack::new();
    for input in graph.input_values_fifo().iter().rev() {
        stack.push(input);
    }

    let mut stack = TrackedStack::new_from_vstack(next_alloc_id, ops_sink, stack, 8);

    let schedule = get_operation_topological_sort(graph);
    for op in schedule {
        schedule_op(config, &mut stack, graph, op);
    }

    shuffle_to_output(config, &mut stack, graph);
}
