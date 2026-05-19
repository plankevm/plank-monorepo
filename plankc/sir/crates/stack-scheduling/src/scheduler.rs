use plank_core::{LoopLimit, list_of_lists::ListOfListsPusher};
use sir_data::{BasicBlockId, BlockView, ControlView, IndexVec, StaticAllocId, index_vec};
use std::{cell::Cell, collections::VecDeque, marker::PhantomData};

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

pub fn dumb_schedule<Sink: FnMut(StackOps)>(
    shuffle_to_output: fn(ScheduleConfig, &mut TrackedStack<'_, Sink>, &OpGraph),
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
        shuffle_to_output(config, &mut stack, graph);
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

pub fn dumb_shuffle_to_output<Sink: FnMut(StackOps)>(
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

fn nth_from_bottom(slice: &[ValueNodeId], n: Height) -> ValueNodeId {
    slice[n.as_depth(slice)]
}

#[derive(Debug, Clone, Copy)]
struct Height(usize);

impl std::ops::Add<usize> for Height {
    type Output = Height;

    fn add(self, rhs: usize) -> Self::Output {
        Height(self.0 + rhs)
    }
}

impl std::ops::AddAssign<usize> for Height {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs
    }
}

impl Height {
    fn as_depth(&self, slice: &[ValueNodeId]) -> usize {
        slice.len() - self.0 - 1
    }
}

pub struct GreedyShuffler<'a, 'ir, Sink: FnMut(StackOps)> {
    stack: &'a mut TrackedStack<'ir, Sink>,
    current_stack: VecDeque<ValueNodeId>,
    target_stack: Vec<ValueNodeId>,
    swap_depth: u8,
    dup_depth: u8,
    limit: LoopLimit,
}

impl<'a, 'ir, Sink: FnMut(StackOps)> GreedyShuffler<'a, 'ir, Sink> {
    fn new(
        stack: &'a mut TrackedStack<'ir, Sink>,
        target_stack: &[ValueNodeId],
        config: ScheduleConfig,
    ) -> Self {
        let current_stack = stack.fifo().iter().copied().collect();
        Self {
            stack,
            current_stack,
            target_stack: Vec::from(target_stack),
            swap_depth: config.max_swap_depth,
            dup_depth: config.max_dup_depth,
            limit: LoopLimit::new(),
        }
    }

    fn pop(&mut self) {
        self.current_stack.pop_front();
        self.stack.pop();
    }

    fn remove_top(&mut self, top: ValueNodeId) {
        assert!(self.current_stack.front().expect("exists") == &top);
        assert!(self.stack.top().expect("exists") == top);
        if !self.target_stack.contains(&top) {
            self.pop();
            return;
        }

        // TODO: questionable whether we want to defer with this check
        let found = match self.current_stack.iter().skip(1).position(|&v| v == top) {
            Some(_) => true,
            None => false,
        };

        if found || self.stack.get_spilled(top).is_some() {
            self.pop();
        } else {
            self.current_stack.pop_front();
            self.stack.spill_top();
        }
    }

    fn run(&mut self) {
        let current_stack_desired_len = self.swap_depth as usize + 1;
        self.shrink_current_stack(current_stack_desired_len);
        self.grow();
    }

    fn swap(&mut self, depth: u8) {
        self.stack.swap(depth);
        self.current_stack.swap(0, depth as usize);
    }

    fn shrink_current_stack(&mut self, current_stack_desired_len: usize) {
        while self.current_stack.len() > current_stack_desired_len {
            self.limit.tick();

            // ignore values at the bottom of the stack that are already in place
            if !self.current_stack.is_empty()
                && !self.target_stack.is_empty()
                && self.current_stack.back() == self.target_stack.last()
            {
                self.current_stack.pop_back();
                self.target_stack.pop();
                continue;
            }

            let top = self.current_stack.front().expect("top must exist");

            // delete top if not in the target stack
            if !self.target_stack.contains(&top) {
                self.pop();
                continue;
            }

            let reachable_bottom_start = if self.target_stack.len() < current_stack_desired_len {
                0
            } else {
                self.target_stack.len() - current_stack_desired_len
            };

            // position of top in the target stack
            let mut target_pos_from_top = 0;
            for (i, value) in self.target_stack.iter().enumerate() {
                if value == top {
                    target_pos_from_top = i;
                    // we break at the first value in "bottom" because in this case, we want to
                    // check if it's as close to the top as possible to swap
                    if target_pos_from_top >= reachable_bottom_start {
                        break;
                    }
                }
            }

            // if top is not part of the "bottom" target stack, we won't be able to do anything with
            // it until we fix the bottom "bottom" means the stack_target_len elements
            // at the bottom of the target stack
            if !(target_pos_from_top >= reachable_bottom_start) {
                self.remove_top(top.clone());
                continue;
            }

            let target_pos_from_bottom = self.target_stack.len() - target_pos_from_top - 1;
            let stack_pos_from_top = self.current_stack.len() - target_pos_from_bottom - 1;
            // place this value in the "bottom" if possible
            if let Ok(depth) = stack_pos_from_top.try_into()
                && depth <= self.swap_depth
            {
                self.swap(depth);
                continue;
            }

            let mut swap_candidate = None;
            for depth in 1..=self.swap_depth as usize {
                let value = self.current_stack[depth];

                if !self.target_stack[reachable_bottom_start..].contains(&value) {
                    swap_candidate = Some((depth, value.clone()));
                    break;
                }
            }

            match swap_candidate {
                Some((i, value)) => {
                    self.swap(i.try_into().expect("valid depth"));
                    self.remove_top(value);
                }
                None => self.remove_top(*top),
            }
        }
    }

    fn grow(&mut self) {}
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
            todo!("greedy shuffle_to_output")
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
    fn shrink_current_stack_does_nothing_when_stack_is_within_swap_depth() {
        let config = ScheduleConfig {
            max_swap_depth: 5,
            max_dup_depth: 4,
            max_exchange_range: 5,
            exchange_cost: 9,
        };
        let initial_stack = [value(1), value(2), value(3), value(4), value(6), value(5)];

        let mut evm_stack = EvmStack::new();
        for &value in initial_stack.iter().rev() {
            evm_stack.push(value);
        }

        let next_alloc_id = Cell::new(StaticAllocId::ZERO);
        let mut ops = ListOfLists::<BasicBlockId, StackOps>::new();
        let block = ops.push_with(|mut pusher| {
            let mut stack =
                TrackedStack::new_from_evm(&next_alloc_id, |op| pusher.push(op), evm_stack, 8);
            let mut shuffler = GreedyShuffler::new(&mut stack, &[], config);
            shuffler.shrink_current_stack(config.max_swap_depth as usize + 1);

            assert_eq!(shuffler.current_stack.iter().copied().collect::<Vec<_>>(), initial_stack);
        });

        assert!(ops[block].is_empty());
    }

    #[test]
    fn shrink_current_stack_pops_top_value_missing_from_target() {
        let config = ScheduleConfig {
            max_swap_depth: 4,
            max_dup_depth: 3,
            max_exchange_range: 4,
            exchange_cost: 9,
        };
        let initial_stack = [value(1), value(2), value(3), value(4), value(6), value(5)];
        let target_stack = [value(2), value(3), value(4), value(5), value(6)];
        let expected_stack = [value(2), value(3), value(4), value(6), value(5)];

        let mut evm_stack = EvmStack::new();
        for &value in initial_stack.iter().rev() {
            evm_stack.push(value);
        }

        let next_alloc_id = Cell::new(StaticAllocId::ZERO);
        let mut ops = ListOfLists::<BasicBlockId, StackOps>::new();
        let block = ops.push_with(|mut pusher| {
            let mut stack =
                TrackedStack::new_from_evm(&next_alloc_id, |op| pusher.push(op), evm_stack, 8);
            let mut shuffler = GreedyShuffler::new(&mut stack, &target_stack, config);
            shuffler.shrink_current_stack(config.max_swap_depth as usize + 1);

            assert_eq!(shuffler.current_stack.iter().copied().collect::<Vec<_>>(), expected_stack);
        });

        assert_eq!(ops[block], [StackOps::Pop]);
    }

    #[test]
    fn shrink_current_stack_places_top_values_then_pops_removable_value() {
        let config = ScheduleConfig {
            max_swap_depth: 4,
            max_dup_depth: 3,
            max_exchange_range: 4,
            exchange_cost: 9,
        };
        let initial_stack = [value(1), value(2), value(3), value(4), value(6), value(5)];
        let target_stack = [value(1), value(2), value(4), value(5), value(6)];
        let expected_stack = [value(1), value(2), value(4), value(6), value(5)];

        let mut evm_stack = EvmStack::new();
        for &value in initial_stack.iter().rev() {
            evm_stack.push(value);
        }

        let next_alloc_id = Cell::new(StaticAllocId::ZERO);
        let mut ops = ListOfLists::<BasicBlockId, StackOps>::new();
        let block = ops.push_with(|mut pusher| {
            let mut stack =
                TrackedStack::new_from_evm(&next_alloc_id, |op| pusher.push(op), evm_stack, 8);
            let mut shuffler = GreedyShuffler::new(&mut stack, &target_stack, config);
            shuffler.shrink_current_stack(config.max_swap_depth as usize + 1);

            assert_eq!(shuffler.current_stack.iter().copied().collect::<Vec<_>>(), expected_stack);
        });

        assert_eq!(ops[block], [StackOps::Swap(1), StackOps::Swap(2), StackOps::Pop]);
    }

    #[test]
    fn shrink_current_stack_spills_and_places_reachable_bottom_values() {
        let config = ScheduleConfig {
            max_swap_depth: 3,
            max_dup_depth: 2,
            max_exchange_range: 3,
            exchange_cost: 9,
        };
        let initial_stack = [value(1), value(2), value(3), value(4), value(5), value(6)];
        let target_stack = [value(1), value(2), value(3), value(4)];
        let expected_stack = [value(1), value(2), value(4), value(6)];

        let mut evm_stack = EvmStack::new();
        for &value in initial_stack.iter().rev() {
            evm_stack.push(value);
        }

        let next_alloc_id = Cell::new(StaticAllocId::ZERO);
        let mut ops = ListOfLists::<BasicBlockId, StackOps>::new();
        let block = ops.push_with(|mut pusher| {
            let mut stack =
                TrackedStack::new_from_evm(&next_alloc_id, |op| pusher.push(op), evm_stack, 8);
            let mut shuffler = GreedyShuffler::new(&mut stack, &target_stack, config);
            shuffler.shrink_current_stack(config.max_swap_depth as usize + 1);

            assert_eq!(shuffler.current_stack.iter().copied().collect::<Vec<_>>(), expected_stack);
        });

        assert_eq!(
            ops[block],
            [
                StackOps::Swap(2),
                StackOps::Store(StaticAllocId::ZERO),
                StackOps::Swap(2),
                StackOps::Swap(3),
                StackOps::Pop,
            ]
        );
    }

    #[test]
    fn shrink_current_stack_places_target_values_while_popping_unneeded_values() {
        let config = ScheduleConfig {
            max_swap_depth: 4,
            max_dup_depth: 3,
            max_exchange_range: 4,
            exchange_cost: 9,
        };
        let initial_stack = [value(1), value(2), value(3), value(4), value(5), value(6)];
        let target_stack = [value(1), value(2), value(3), value(4)];
        let expected_stack = [value(2), value(1), value(4), value(3), value(6)];

        let mut evm_stack = EvmStack::new();
        for &value in initial_stack.iter().rev() {
            evm_stack.push(value);
        }

        let next_alloc_id = Cell::new(StaticAllocId::ZERO);
        let mut ops = ListOfLists::<BasicBlockId, StackOps>::new();
        let block = ops.push_with(|mut pusher| {
            let mut stack =
                TrackedStack::new_from_evm(&next_alloc_id, |op| pusher.push(op), evm_stack, 8);
            let mut shuffler = GreedyShuffler::new(&mut stack, &target_stack, config);
            shuffler.shrink_current_stack(config.max_swap_depth as usize + 1);

            assert_eq!(shuffler.current_stack.iter().copied().collect::<Vec<_>>(), expected_stack);
        });

        assert_eq!(ops[block], [StackOps::Swap(2), StackOps::Swap(4), StackOps::Pop,]);
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
