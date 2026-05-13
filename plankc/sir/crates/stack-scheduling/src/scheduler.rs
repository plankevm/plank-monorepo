use std::cell::Cell;

use plank_core::list_of_lists::ListOfListsPusher;
use sir_data::{BasicBlockId, BlockView, ControlView, IndexVec, StaticAllocId, index_vec};

use crate::{
    op_graph::*,
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
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

    let mut in_the_way_buf = Vec::new();

    for &input in graph.operations[op].consumes_fifo.iter().rev() {
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
        if target_counts[top] == 0 || stack.get_spilled(top).is_some() {
            stack.pop();
        } else {
            stack.spill_top();
        }
    }

    for &target in target_stack.iter().rev() {
        let slot = stack.get_spilled(target).expect("missing value in spilled");
        stack.load(slot);
    }
}

pub fn dumb_schedule(
    ops_sink: &mut ListOfListsPusher<'_, BasicBlockId, StackOps>,
    block: BlockView<'_>,
    next_alloc_id: &Cell<StaticAllocId>,
    config: ScheduleConfig,
    graph: &OpGraph,
) {
    let mut stack = EvmStack::new();
    for input in graph.input_values_fifo().iter().rev() {
        stack.push(input);
    }

    let mut stack = TrackedStack::new_from_evm(next_alloc_id, ops_sink, stack, 8);

    let schedule = get_operation_topological_sort(graph);
    for op in schedule {
        schedule_op(config, &mut stack, graph, op);
    }

    if !matches!(block.control(), ControlView::LastOpTerminates) {
        shuffle_to_output(config, &mut stack, graph);
    }
}
