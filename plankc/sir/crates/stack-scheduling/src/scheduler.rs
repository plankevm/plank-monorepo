use plank_core::{IndexVec, index_vec};
use sir_data::{BlockView, ControlView, StaticAllocId};
use std::cell::Cell;

use crate::{
    op_graph::{OpGraph, OpNodeId, ValueNodeId},
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};

fn get_operation_topological_sort(graph: &OpGraph) -> Vec<OpNodeId> {
    let mut in_degrees: IndexVec<OpNodeId, u32> = index_vec![0; graph.total_ops() as usize];
    for id in graph.op_ids() {
        in_degrees[id] = graph.get_predecessors(id).count_members();
    }

    let mut topo_sort = Vec::with_capacity(graph.total_ops() as usize);
    let mut queue = Vec::with_capacity(128);

    for id in graph.op_ids() {
        if in_degrees[id] == 0 {
            queue.push(id);
        }
    }

    while let Some(id) = queue.pop() {
        topo_sort.push(id);
        for succ in graph.op_ids() {
            if graph.get_predecessors(succ).contains(id) {
                in_degrees[succ] -= 1;
                if in_degrees[succ] == 0 {
                    queue.push(succ);
                }
            }
        }
    }

    topo_sort
}

// dumb intra-instruction scheduler that always dups its inputs.
fn schedule_op<Sink: FnMut(StackOps)>(
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

fn count_occurences(values: &[ValueNodeId], total_values: usize) -> IndexVec<ValueNodeId, u16> {
    let mut counts = IndexVec::new();
    counts.resize(total_values, 0);
    for &value in values {
        counts[value] += 1;
    }
    counts
}

fn shuffle_to_output<Sink: FnMut(StackOps)>(
    _config: ScheduleConfig,
    stack: &mut TrackedStack<'_, Sink>,
    graph: &OpGraph,
) {
    let target_stack = graph.output_values_fifo();
    let target_counts = count_occurences(target_stack, graph.total_values() as usize);

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
    ops_sink: impl FnMut(StackOps),
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
    let mut in_the_way_buf = Vec::new();
    for op in schedule {
        schedule_op(config, &mut stack, graph, op, &mut in_the_way_buf);
    }

    if !matches!(block.control(), ControlView::LastOpTerminates) {
        shuffle_to_output(config, &mut stack, graph);
    }
}
