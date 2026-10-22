//! Optimal scheduling of small graphs by A* over (stack contents, completed operations) states.
//!
//! Moves are `swap`, `dup` and `pop` (one unit each, all cost the same gas) and executing a
//! ready operation whose operands already sit on top of the stack (free, every schedule runs
//! each operation once). The heuristic counts the copies every live value still lacks plus the
//! surplus copies that must eventually be popped; each costs at least one move and an operation
//! never lowers it, so the first goal popped from the queue is optimal among spill-free
//! schedules that keep the stack within `dup`/`swap` reach.

use std::{cmp::Reverse, collections::BinaryHeap, time::Instant};

use hashbrown::{HashMap, hash_map::Entry};
use plank_core::Idx;
use sir_data::StaticAllocId;

use crate::{
    BlockFinalization,
    op_graph::{OpGraph, OpNodeId, OpNodeKind},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack},
};

const MAX_STACK: usize = 17;
/// Larger graphs rarely finish within budget and the time is better spent on the heuristic
/// search.
const MAX_OPS: u32 = 10;
/// Block boundaries close to `dup`/`swap` reach make the spill-free optimum worse than schedules
/// that briefly exceed it.
const MAX_BOUNDARY_VALUES: usize = 12;
const MAX_VALUES: u32 = 255;
const DEADLINE_CHECK_INTERVAL: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct State {
    done: u64,
    len: u8,
    /// Value indices, top of the stack first.
    stack: [u8; MAX_STACK],
}

impl State {
    fn fifo(&self) -> &[u8] {
        &self.stack[..usize::from(self.len)]
    }
}

#[derive(Clone, Copy)]
enum Move {
    Start,
    Swap(u8),
    Dup(u8),
    Pop,
    Op(OpNodeId, bool),
}

struct Node {
    state: State,
    parent: u32,
    step: Move,
}

struct Problem<'g> {
    graph: &'g OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    /// Per value: operations reading it, with how many operand slots they read it in.
    readers: Vec<Vec<(u64, u32)>>,
    output_uses: Vec<u32>,
    /// Per value: operations that must have run for the value to exist (0 for block inputs).
    producer_bit: Vec<u64>,
    predecessors: Vec<u64>,
    inputs: Vec<Vec<u8>>,
    outputs: Vec<Vec<u8>>,
    flippable: Vec<bool>,
    all_done: u64,
    goal_stack: Vec<u8>,
    /// Per value used exactly once in the outputs: how many values sit below it in the goal
    /// stack. `u8::MAX` otherwise.
    goal_height: Vec<u8>,
    surplus_dups: bool,
}

impl Problem<'_> {
    fn needed(&self, value: usize, done: u64) -> u32 {
        self.output_uses[value]
            + self.readers[value]
                .iter()
                .filter(|&&(bit, _)| done & bit == 0)
                .map(|&(_, slots)| slots)
                .sum::<u32>()
    }

    /// Lower bound on the remaining stack moves, `None` when a needed value is lost for good.
    fn estimate(&self, state: &State) -> Option<u32> {
        let mut copies = [0u32; MAX_VALUES as usize];
        for &value in state.fifo() {
            copies[usize::from(value)] += 1;
        }
        let mut bound = 0;
        let mut misplaced_pass_through = 0u32;
        if self.finalization == BlockFinalization::ShuffleToOutputs {
            let len = state.fifo().len();
            for (position, &value) in state.fifo().iter().enumerate() {
                let value = usize::from(value);
                // Nothing else consumes such a value and a single copy cannot be popped, so the
                // number of values below it only changes when a swap moves it, and each swap
                // moves two values.
                if self.goal_height[value] != u8::MAX
                    && copies[value] == 1
                    && self.needed(value, state.done) == 1
                    && usize::from(self.goal_height[value]) != len - 1 - position
                {
                    misplaced_pass_through += 1;
                }
            }
        }
        for value in 0..self.readers.len() {
            let produced = state.done & self.producer_bit[value] == self.producer_bit[value];
            if !produced {
                continue;
            }
            let needed = self.needed(value, state.done);
            let have = copies[value];
            if needed > 0 && have == 0 {
                return None;
            }
            bound += needed.saturating_sub(have);
            if self.finalization == BlockFinalization::ShuffleToOutputs {
                bound += have.saturating_sub(needed);
            }
        }
        if bound == 0 && state.done == self.all_done {
            // Only the permutation is left: a swap fixes at most one position below the top.
            let misplaced = state
                .fifo()
                .iter()
                .zip(&self.goal_stack)
                .skip(1)
                .filter(|(have, want)| have != want)
                .count();
            return Some((misplaced as u32).max(misplaced_pass_through.div_ceil(2)));
        }
        Some(bound + misplaced_pass_through.div_ceil(2))
    }

    fn is_goal(&self, state: &State) -> bool {
        state.done == self.all_done
            && (self.finalization == BlockFinalization::LastOpTerminates
                || state.fifo() == self.goal_stack.as_slice())
    }
}

/// Returns an optimal spill-free schedule with fewer than `move_limit` stack moves, or `None` if
/// the graph is outside the supported size, there is no such schedule or the search does not
/// finish before `deadline`. Without `surplus_dups` a value is never duplicated beyond its
/// remaining uses, which shrinks the search a lot but can miss the optimum.
pub fn exact_schedule(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    next_alloc_id: StaticAllocId,
    surplus_dups: bool,
    move_limit: u32,
    deadline: Instant,
) -> Option<Vec<StackOps>> {
    let input_count = graph.input_values_fifo().len() as usize;
    if graph.total_ops() > MAX_OPS
        || graph.total_values() > MAX_VALUES
        || input_count > MAX_BOUNDARY_VALUES
        || graph.output_values_fifo().len() > MAX_BOUNDARY_VALUES
    {
        return None;
    }
    let problem = build_problem(graph, finalization, config, surplus_dups);
    let start = {
        let mut stack = [0u8; MAX_STACK];
        for (slot, value) in stack.iter_mut().zip(graph.input_values_fifo().iter()) {
            *slot = value.idx() as u8;
        }
        State { done: 0, len: input_count as u8, stack }
    };

    let mut nodes = vec![Node { state: start, parent: 0, step: Move::Start }];
    let mut best_cost = HashMap::new();
    best_cost.insert(start, 0u32);
    let mut queue = BinaryHeap::new();
    queue.push((Reverse(problem.estimate(&start)?), 0u32, 0u32));
    let mut expanded = 0usize;

    while let Some((_, cost, index)) = queue.pop() {
        let state = nodes[index as usize].state;
        if best_cost.get(&state).is_some_and(|&best| best < cost) {
            continue;
        }
        if problem.is_goal(&state) {
            return Some(replay(graph, next_alloc_id, &nodes, index));
        }
        expanded += 1;
        if expanded % DEADLINE_CHECK_INTERVAL == 0 && Instant::now() >= deadline {
            return None;
        }
        for_each_successor(&problem, &state, |next, step, step_cost| {
            let next_cost = cost + step_cost;
            match best_cost.entry(next) {
                Entry::Occupied(mut entry) => {
                    if *entry.get() <= next_cost {
                        return;
                    }
                    entry.insert(next_cost);
                }
                Entry::Vacant(entry) => {
                    entry.insert(next_cost);
                }
            }
            let Some(estimate) = problem.estimate(&next) else { return };
            if next_cost + estimate >= move_limit {
                return;
            }
            nodes.push(Node { state: next, parent: index, step });
            // Among equal estimates, deeper nodes first: they are closer to a goal.
            queue.push((Reverse(next_cost + estimate), next_cost, (nodes.len() - 1) as u32));
        });
    }
    None
}

fn build_problem(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    surplus_dups: bool,
) -> Problem<'_> {
    let values = graph.total_values() as usize;
    let mut readers = vec![Vec::<(u64, u32)>::new(); values];
    let mut producer_bit = vec![0u64; values];
    let mut predecessors = Vec::new();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut flippable = Vec::new();
    for op in graph.op_ids() {
        let view = graph.get_op(op);
        let bit = 1u64 << op.idx();
        for &value in view.inputs_fifo {
            let entry = &mut readers[value.idx()];
            match entry.iter_mut().find(|(reader, _)| *reader == bit) {
                Some((_, slots)) => *slots += 1,
                None => entry.push((bit, 1)),
            }
        }
        for &value in view.outputs_fifo {
            producer_bit[value.idx()] = bit;
        }
        predecessors.push(view.predecessors.iter().fold(0u64, |mask, pred| mask | 1 << pred.idx()));
        inputs.push(view.inputs_fifo.iter().map(|value| value.idx() as u8).collect());
        outputs.push(view.outputs_fifo.iter().map(|value| value.idx() as u8).collect());
        flippable.push(matches!(view.kind, OpNodeKind::Flippable(_)));
    }
    let mut output_uses = vec![0u32; values];
    for &value in graph.output_values_fifo() {
        output_uses[value.idx()] += 1;
    }
    let all_done = (1u64 << graph.total_ops()) - 1;
    let outputs_len = graph.output_values_fifo().len();
    let mut goal_height = vec![u8::MAX; values];
    for (position, &value) in graph.output_values_fifo().iter().enumerate() {
        if output_uses[value.idx()] == 1 {
            goal_height[value.idx()] = (outputs_len - 1 - position) as u8;
        }
    }
    Problem {
        graph,
        finalization,
        config,
        readers,
        output_uses,
        producer_bit,
        predecessors,
        inputs,
        outputs,
        flippable,
        all_done,
        goal_stack: graph.output_values_fifo().iter().map(|value| value.idx() as u8).collect(),
        goal_height,
        surplus_dups,
    }
}

fn for_each_successor(
    problem: &Problem<'_>,
    state: &State,
    mut visit: impl FnMut(State, Move, u32),
) {
    let fifo = state.fifo();
    let len = fifo.len();
    for op in problem.graph.op_ids() {
        let bit = 1u64 << op.idx();
        let index = op.idx();
        if state.done & bit != 0
            || state.done & problem.predecessors[index] != problem.predecessors[index]
        {
            continue;
        }
        let inputs = &problem.inputs[index];
        let orientations: &[bool] = if problem.flippable[index] && inputs[0] != inputs[1] {
            &[false, true]
        } else {
            &[false]
        };
        for &flipped in orientations {
            if len < inputs.len() {
                continue;
            }
            let matches = inputs.iter().enumerate().all(|(slot, &value)| {
                let slot = if flipped && slot < 2 { 1 - slot } else { slot };
                fifo[slot] == value
            });
            if !matches {
                continue;
            }
            let outputs = &problem.outputs[index];
            let new_len = len - inputs.len() + outputs.len();
            if new_len > MAX_STACK {
                continue;
            }
            let mut next =
                State { done: state.done | bit, len: new_len as u8, stack: [0; MAX_STACK] };
            next.stack[..outputs.len()].copy_from_slice(outputs);
            next.stack[outputs.len()..new_len].copy_from_slice(&fifo[inputs.len()..]);
            visit(next, Move::Op(op, flipped), 0);
        }
    }

    let surplus = |value: u8, copies: u32| copies > problem.needed(usize::from(value), state.done);
    let copies_of = |value: u8| fifo.iter().filter(|&&other| other == value).count() as u32;

    if len < MAX_STACK {
        for depth in 0..len.min(usize::from(problem.config.max_dup_depth) + 1) {
            let value = fifo[depth];
            // One surplus copy can pay off: duplicating a deep last use and popping the stale
            // original later is often cheaper than digging the original up.
            let copies_after =
                if problem.surplus_dups { copies_of(value) } else { copies_of(value) + 1 };
            if surplus(value, copies_after) {
                continue;
            }
            let mut next = *state;
            next.len += 1;
            next.stack[0] = value;
            next.stack[1..=len].copy_from_slice(fifo);
            visit(next, Move::Dup(depth as u8), 1);
        }
    }
    for depth in 1..len.min(usize::from(problem.config.max_swap_depth) + 1) {
        if fifo[depth] == fifo[0] {
            continue;
        }
        let mut next = *state;
        next.stack.swap(0, depth);
        visit(next, Move::Swap(depth as u8), 1);
    }
    if len > 0 && surplus(fifo[0], copies_of(fifo[0])) {
        let mut next = *state;
        next.len -= 1;
        next.stack.copy_within(1..len, 0);
        next.stack[len - 1] = 0;
        visit(next, Move::Pop, 1);
    }
}

fn replay(
    graph: &OpGraph,
    next_alloc_id: StaticAllocId,
    nodes: &[Node],
    goal: u32,
) -> Vec<StackOps> {
    let mut steps = Vec::new();
    let mut index = goal;
    while index != 0 {
        steps.push(nodes[index as usize].step);
        index = nodes[index as usize].parent;
    }
    steps.reverse();

    let mut ops = Vec::new();
    let mut inner = EvmStack::new();
    for input in graph.input_values_fifo().iter().rev() {
        inner.push(input);
    }
    let mut stack = TrackedStack::new_from_evm(next_alloc_id, |op| ops.push(op), inner, 0);
    for step in steps {
        match step {
            Move::Start => unreachable!("only the root node is a start"),
            Move::Swap(depth) => stack.swap(depth),
            Move::Dup(depth) => stack.dup(depth),
            Move::Pop => stack.pop(),
            Move::Op(op, flipped) => stack.op(graph, op, flipped),
        }
    }
    drop(stack);
    ops
}
