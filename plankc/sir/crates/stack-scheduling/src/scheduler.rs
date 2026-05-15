use plank_core::{LoopLimit, list_of_lists::ListOfListsPusher};
use sir_data::{BasicBlockId, BlockView, ControlView, IndexVec, StaticAllocId, index_vec};
use std::{cell::Cell, marker::PhantomData};

use crate::{
    op_graph::*,
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};

pub trait Scheduler {
    fn schedule<Sink: FnMut(StackOps)>(
        ops_sink: Sink,
        block: BlockView<'_>,
        next_alloc_id: &Cell<StaticAllocId>,
        config: ScheduleConfig,
        graph: &OpGraph,
    );
}

pub struct DumbScheduler<S: Shuffler = DumbShuffler>(PhantomData<S>);

impl<S: Shuffler> Scheduler for DumbScheduler<S> {
    fn schedule<Sink: FnMut(StackOps)>(
        ops_sink: Sink,
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
            DumbOpScheduler::schedule_op(config, &mut stack, graph, op);
        }

        if !matches!(block.control(), ControlView::LastOpTerminates) {
            S::shuffle_to_output(config, &mut stack, graph);
        }
    }
}

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

pub trait OpScheduler {
    fn schedule_op<Sink: FnMut(StackOps)>(
        config: ScheduleConfig,
        stack: &mut TrackedStack<'_, Sink>,
        graph: &OpGraph,
        op: OpNodeId,
    );
}

pub struct DumbOpScheduler;

impl OpScheduler for DumbOpScheduler {
    fn schedule_op<Sink: FnMut(StackOps)>(
        config: ScheduleConfig,
        stack: &mut TrackedStack<'_, Sink>,
        graph: &OpGraph,
        op: OpNodeId,
    ) {
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
}

fn count_occurences(values: &[ValueNodeId], total_values: usize) -> IndexVec<ValueNodeId, u16> {
    let mut counts = IndexVec::new();
    counts.resize(total_values, 0);
    for &value in values {
        counts[value] += 1;
    }
    counts
}

pub trait Shuffler {
    fn shuffle_to_output<Sink: FnMut(StackOps)>(
        config: ScheduleConfig,
        stack: &mut TrackedStack<'_, Sink>,
        graph: &OpGraph,
    );
}

pub struct DumbShuffler;

impl Shuffler for DumbShuffler {
    fn shuffle_to_output<Sink: FnMut(StackOps)>(
        _config: ScheduleConfig,
        stack: &mut TrackedStack<'_, Sink>,
        graph: &OpGraph,
    ) {
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
}

pub struct GreedyShuffler;

impl Shuffler for GreedyShuffler {
    fn shuffle_to_output<Sink: FnMut(StackOps)>(
        config: ScheduleConfig,
        stack: &mut TrackedStack<'_, Sink>,
        graph: &OpGraph,
    ) {
        let target_stack = graph.end_stack_fifo.as_slice();
        if target_stack.is_empty() {
            while !stack.is_empty() {
                stack.pop();
            }
            return;
        }

        let mut target_counts = count_occurences(target_stack, graph.values.len());

        let max_depth = config.max_swap_depth as usize;

        let mut limit = LoopLimit::new();
        while stack.len() as usize > max_depth + 1 {
            limit.tick();
            let top = stack.top().expect("top should exist");
            if target_counts[top] != 0 && stack.get_spilled(top).is_none() {
                stack.spill_top();
            } else {
                stack.pop();
            }
        }

        'rearrange: loop {
            limit.tick();
            let stack_depth = stack.len() as usize - 1;
            let target_depth = target_stack.len() - 1;
            let reachable_depth = stack_depth.min(target_depth).min(max_depth);
            let top = stack.top().expect("top should exist");
            if let Some(pos) = target_stack[..=reachable_depth].iter().position(|&v| v == top)
                && pos != 0
                && stack.fifo()[pos] != top
            {
                stack.swap(pos.try_into().expect("valid pos"));
            } else {
                for depth in 1..=reachable_depth {
                    let value = stack.fifo()[depth];

                    if value == top {
                        continue;
                    }

                    let Some(pos) =
                        target_stack[..=reachable_depth].iter().position(|&v| v == value)
                    else {
                        continue;
                    };
                    if pos == depth || stack.fifo()[pos] == value {
                        continue;
                    }
                    stack.swap(depth.try_into().expect("valid depth"));
                    if pos != 0 {
                        stack.swap(pos.try_into().expect("valid pos"));
                    }

                    continue 'rearrange;
                }
                break;
            }
        }

        let mut placed = 0;

        while placed < target_stack.len() {
            limit.tick();

            let remaining_target_len = target_stack.len() - placed;
            let remaining_stack_len = stack.len() as usize - placed;
            let target_depth = remaining_target_len - 1;
            let needed = target_stack[target_depth];

            if remaining_stack_len == 0 {
                let slot = stack.get_spilled(needed).expect("needed value missing in spilled");
                stack.load(slot);
                continue;
            }

            let stack_depth = remaining_stack_len - 1;

            if stack.fifo()[stack_depth] == needed {
                target_counts[needed] -= 1;
                if target_counts[needed] > 0 && stack.get_spilled(needed).is_none() {
                    stack.dup(stack_depth.try_into().expect("valid stack_depth"));
                }
                placed += 1;
                continue;
            }

            if let Some(pos) = stack.fifo()[..stack_depth].iter().position(|&v| v == needed) {
                if pos != 0 {
                    stack.swap(pos.try_into().expect("valid pos"));
                }
                stack.swap(stack_depth.try_into().expect("valid stack depth"));
            } else {
                // value not on the stack, need to load it
                if stack.len() == max_depth.try_into().expect("max depth is u16") {
                    let top = stack.top().expect("top should exist");
                    if target_counts[top] == 0 || stack.get_spilled(top).is_some() {
                        stack.pop();
                    } else {
                        stack.spill_top();
                    }
                }

                let slot = stack.get_spilled(needed).expect("needed value missing in spilled");
                stack.load(slot);
                stack.swap((stack_depth + 1).try_into().expect("valid stack_depth"));
            }

            target_counts[needed] -= 1;
            if target_counts[needed] > 0 && stack.get_spilled(needed).is_none() {
                stack.dup(stack_depth.try_into().expect("valid stack_depth"));
            }
            placed += 1;
        }

        while stack.len() as usize > target_stack.len() {
            limit.tick();
            stack.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use plank_core::list_of_lists::ListOfLists;
    use sir_data::{BasicBlockId, Idx, IndexVec, StaticAllocId};

    use super::*;

    fn value(id: u32) -> ValueNodeId {
        ValueNodeId::new(id)
    }

    fn graph_with_target(value_count: usize, end_stack_fifo: Vec<ValueNodeId>) -> OpGraph {
        let mut values = IndexVec::<ValueNodeId, ValueNode>::new();
        for _ in 0..value_count {
            values.push(ValueNode { source: None, used_by: Vec::new() });
        }

        OpGraph {
            operations: IndexVec::new(),
            values,
            inputs_end: ValueNodeId::ZERO,
            end_stack_fifo,
        }
    }

    fn greedy_shuffle_ops(
        config: ScheduleConfig,
        initial_stack_top_first: &[ValueNodeId],
        target_stack_top_first: Vec<ValueNodeId>,
    ) -> Vec<StackOps> {
        let mut evm_stack = EvmStack::new();
        for &value in initial_stack_top_first.iter().rev() {
            evm_stack.push(value);
        }

        let graph = graph_with_target(3, target_stack_top_first);
        let next_alloc_id = Cell::new(StaticAllocId::ZERO);
        let mut ops = ListOfLists::<BasicBlockId, StackOps>::new();
        let block = ops.push_with(|mut pusher| {
            let mut stack =
                TrackedStack::new_from_evm(&next_alloc_id, |op| pusher.push(op), evm_stack, 8);
            <GreedyShuffler as Shuffler>::shuffle_to_output(config, &mut stack, &graph);
        });

        ops[block].to_vec()
    }

    fn replay_shuffle_ops(
        initial_stack_top_first: &[ValueNodeId],
        ops: &[StackOps],
    ) -> Vec<ValueNodeId> {
        let mut stack = initial_stack_top_first.to_vec();
        let mut static_allocs = Vec::<(StaticAllocId, ValueNodeId)>::new();

        for &op in ops {
            match op {
                StackOps::Swap(depth) => stack.swap(0, depth as usize),
                StackOps::Dup(depth) => stack.insert(0, stack[depth as usize]),
                StackOps::Pop => {
                    stack.remove(0);
                }
                StackOps::Store(alloc) => static_allocs.push((alloc, stack.remove(0))),
                StackOps::Load(alloc) => {
                    let &(_, value) = static_allocs
                        .iter()
                        .find(|&&(stored_alloc, _)| stored_alloc == alloc)
                        .expect("load should reference previous store");
                    stack.insert(0, value);
                }
                StackOps::Op(_) | StackOps::CallRetPush(_) | StackOps::Exchange(_, _) => {
                    panic!("unexpected op in shuffler test")
                }
            }
        }

        stack
    }

    #[test]
    fn greedy_spills_until_deep_target_value_is_reachable() {
        let config = ScheduleConfig {
            max_swap_depth: 1,
            max_dup_depth: 1,
            max_exchange_range: 1,
            exchange_cost: 9,
        };
        let a = value(0);
        let b = value(1);
        let c = value(2);

        let initial_stack = [c, b, a];
        let target_stack = vec![b];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_handles_target_shorter_than_swap_window() {
        let config = ScheduleConfig::default();
        let a = value(0);
        let b = value(1);

        let initial_stack = [a, b];
        let target_stack = vec![b, a];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_duplicates_needed_value_while_reordering() {
        let config = ScheduleConfig::default();
        let a = value(0);
        let b = value(1);
        let c = value(2);

        let initial_stack = [c, b, a];
        let target_stack = vec![b, c, b];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert!(ops.iter().any(|op| matches!(op, StackOps::Dup(_))));
        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_does_not_loop_when_top_value_has_multiple_target_positions() {
        let config = ScheduleConfig::default();
        let a = value(0);
        let b = value(1);

        let initial_stack = [a, a, b];
        let target_stack = vec![a, b, a];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_rearranges_repeated_values_when_first_target_match_is_already_correct() {
        let config = ScheduleConfig::default();
        let a = value(0);
        let b = value(1);

        let initial_stack = [a, b, b, a, a];
        let target_stack = vec![a, b, a, b, a];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_rearranges_with_stack_deeper_than_target() {
        let config = ScheduleConfig::default();
        let a = value(0);
        let b = value(1);
        let c = value(2);

        let initial_stack = [a, b, a, c, a];
        let target_stack = vec![a, c, a];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_can_extend_stack_with_duplicate() {
        let config = ScheduleConfig::default();
        let a = value(0);

        let initial_stack = [a];
        let target_stack = vec![a, a];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert!(ops.iter().any(|op| matches!(op, StackOps::Dup(_))));
        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_handles_missing_value_after_current_stack_is_placed() {
        let config = ScheduleConfig {
            max_swap_depth: 1,
            max_dup_depth: 1,
            max_exchange_range: 1,
            exchange_cost: 9,
        };
        let a = value(0);
        let b = value(1);
        let c = value(2);

        let initial_stack = [b, c, a];
        let target_stack = vec![b, a];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_pops_everything_for_empty_target() {
        let config = ScheduleConfig::default();
        let a = value(0);
        let b = value(1);
        let c = value(2);

        let initial_stack = [a, b, c];
        let target_stack = vec![];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_uses_max_depth_swap_without_spilling() {
        let config = ScheduleConfig {
            max_swap_depth: 1,
            max_dup_depth: 1,
            max_exchange_range: 1,
            exchange_cost: 9,
        };
        let a = value(0);
        let b = value(1);

        let initial_stack = [a, b];
        let target_stack = vec![b, a];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert_eq!(ops, vec![StackOps::Swap(1)]);
        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }

    #[test]
    fn greedy_spills_duplicated_value_to_make_room_for_load() {
        let config = ScheduleConfig {
            max_swap_depth: 1,
            max_dup_depth: 1,
            max_exchange_range: 1,
            exchange_cost: 9,
        };
        let a = value(0);
        let b = value(1);
        let c = value(2);

        let initial_stack = [b, c, a];
        let target_stack = vec![a, b, a];
        let ops = greedy_shuffle_ops(config, &initial_stack, target_stack.clone());

        assert!(ops.iter().any(|op| matches!(op, StackOps::Dup(_))));
        assert!(ops.iter().any(|op| matches!(op, StackOps::Load(_))));
        assert_eq!(replay_shuffle_ops(&initial_stack, &ops), target_stack);
    }
}
