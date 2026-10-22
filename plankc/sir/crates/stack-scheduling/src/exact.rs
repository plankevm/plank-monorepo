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
    op_graph::{OpGraph, OpNodeId, OpNodeKind, ValueNodeId},
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
const DEADLINE_CHECK_INTERVAL: usize = 32;

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

/// Values and operations are numbered locally: value `i` is the problem's `i`-th value, and the
/// operation at `ops[i]` owns bit `i` of `done`. Operations outside `ops` count as already run.
struct Problem {
    ops: Vec<OpNodeId>,
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
    goal_done: u64,
    goal_stack: Vec<u8>,
    /// Per value used exactly once in the outputs: how many values sit below it in the goal
    /// stack. `u8::MAX` otherwise.
    goal_height: Vec<u8>,
    /// Per value: its slot in the goal stack, `None` when the goal holds a value twice.
    goal_position: Option<Vec<u8>>,
    surplus_dups: bool,
}

impl Problem {
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
        let mut copies = [0u8; MAX_VALUES as usize];
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
                // The producer creates exactly one copy, so every further use needs a dup and an
                // unused result needs a pop. Producers outside the goal never run.
                if self.goal_done & self.producer_bit[value] == self.producer_bit[value] {
                    let needed = self.needed(value, state.done);
                    bound += needed.saturating_sub(1);
                    if needed == 0 && self.finalization == BlockFinalization::ShuffleToOutputs {
                        bound += 1;
                    }
                }
                continue;
            }
            let needed = self.needed(value, state.done);
            let have = u32::from(copies[value]);
            if needed > 0 && have == 0 {
                return None;
            }
            bound += needed.saturating_sub(have);
            if self.finalization == BlockFinalization::ShuffleToOutputs {
                bound += have.saturating_sub(needed);
            }
        }
        if bound == 0 && state.done == self.goal_done {
            // Only the permutation is left: a swap fixes at most one position below the top.
            let misplaced = state
                .fifo()
                .iter()
                .zip(&self.goal_stack)
                .skip(1)
                .filter(|(have, want)| have != want)
                .count() as u32;
            return Some(
                (misplaced + self.cycles_below_top(state)).max(misplaced_pass_through.div_ceil(2)),
            );
        }
        Some(bound + misplaced_pass_through.div_ceil(2))
    }

    /// Permutation cycles not containing the top: each needs one swap beyond its length to bring
    /// a member to the top first. Only defined when the goal holds distinct values.
    fn cycles_below_top(&self, state: &State) -> u32 {
        let Some(goal_position) = &self.goal_position else { return 0 };
        let stack = state.fifo();
        if stack.len() != self.goal_stack.len() {
            return 0;
        }
        let mut visited = 0u32;
        let mut cycles = 0;
        for start in 1..stack.len() {
            if visited & (1 << start) != 0 || stack[start] == self.goal_stack[start] {
                continue;
            }
            let mut position = start;
            let mut contains_top = false;
            while visited & (1 << position) == 0 {
                visited |= 1 << position;
                contains_top |= position == 0;
                position = usize::from(goal_position[usize::from(stack[position])]);
            }
            if !contains_top {
                cycles += 1;
            }
        }
        cycles
    }

    fn is_goal(&self, state: &State) -> bool {
        state.done == self.goal_done
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
    let ops: Vec<OpNodeId> = graph.op_ids().collect();
    let (problem, start) = build_problem(
        graph,
        finalization,
        config,
        surplus_dups,
        ops,
        input_stack(graph).fifo(),
        graph.output_values_fifo(),
    )?;

    let (nodes, goal) = astar(&problem, start, move_limit, deadline)?;
    Some(replay(graph, next_alloc_id, &nodes, goal))
}

fn astar(
    problem: &Problem,
    start: State,
    move_limit: u32,
    deadline: Instant,
) -> Option<(Vec<Node>, u32)> {
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
            return Some((nodes, index));
        }
        expanded += 1;
        if expanded.is_multiple_of(DEADLINE_CHECK_INTERVAL) && Instant::now() >= deadline {
            return None;
        }
        for_each_successor(problem, &state, |next, step, step_cost| {
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

/// The beam only beats the policy search on graphs slightly beyond the exact A*'s reach.
pub const MAX_BEAM_OPS: u32 = 16;
/// Ops beyond this no longer fit the `done` bitmask.
const MAX_MASK_OPS: u32 = 63;
/// The beam's result is only a candidate next to the policy searches, so unlike the A* it also
/// runs on boundaries close to `dup`/`swap` reach.
const MAX_BEAM_BOUNDARY_VALUES: usize = MAX_STACK - 1;
const FIRST_BEAM_WIDTH: usize = 4;
/// Operands-ready operations can be executed in any interleaving, so the free closure of a layer
/// is capped at this many states per beam slot.
const CLOSURE_PER_WIDTH: usize = 8;

/// Anytime beam search over the same states and moves as [`exact_schedule`] for graphs the A*
/// cannot finish: layer `k` holds states reached with `k` stack moves (operations are free and
/// join the layer they are executed in), only the `width` states with the smallest lower bound
/// are expanded further. Reruns with doubled width until `deadline`, keeping the cheapest
/// schedule. Returns `None` if no beam reached the goal within `move_limit` moves.
pub fn beam_schedule(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    next_alloc_id: StaticAllocId,
    mut move_limit: u32,
    deadline: Instant,
) -> Option<Vec<StackOps>> {
    let input_count = graph.input_values_fifo().len() as usize;
    if graph.total_ops() > MAX_BEAM_OPS
        || graph.total_values() > MAX_VALUES
        || input_count > MAX_BEAM_BOUNDARY_VALUES
        || graph.output_values_fifo().len() > MAX_BEAM_BOUNDARY_VALUES
    {
        return None;
    }
    let ops: Vec<OpNodeId> = graph.op_ids().collect();
    let (problem, start) = build_problem(
        graph,
        finalization,
        config,
        false,
        ops,
        input_stack(graph).fifo(),
        graph.output_values_fifo(),
    )?;
    problem.estimate(&start)?;
    let mut best = None;
    let mut width = FIRST_BEAM_WIDTH;
    loop {
        match beam_pass(&problem, start, width, move_limit, deadline) {
            BeamPass::Found(moves, nodes, goal) => {
                move_limit = moves;
                best = Some(replay(graph, next_alloc_id, &nodes, goal));
            }
            BeamPass::NotFound => {}
            BeamPass::OutOfTime => return best,
        }
        if Instant::now() >= deadline || move_limit == 0 {
            return best;
        }
        width = width.saturating_mul(2);
    }
}

enum BeamPass {
    Found(u32, Vec<Node>, u32),
    NotFound,
    OutOfTime,
}

fn beam_pass(
    problem: &Problem,
    start: State,
    width: usize,
    move_limit: u32,
    deadline: Instant,
) -> BeamPass {
    let mut nodes = vec![Node { state: start, parent: 0, step: Move::Start }];
    let mut seen = hashbrown::HashSet::new();
    seen.insert(start);
    let mut layer = vec![0u32];
    let mut ranked = Vec::new();
    let mut expanded = 0usize;
    for moves in 0..move_limit {
        let mut cursor = 0;
        let closure_cap = layer.len() + CLOSURE_PER_WIDTH * width;
        while cursor < layer.len() {
            let index = layer[cursor];
            cursor += 1;
            let state = nodes[index as usize].state;
            if problem.is_goal(&state) {
                return BeamPass::Found(moves, nodes, index);
            }
            if cursor.is_multiple_of(DEADLINE_CHECK_INTERVAL) && Instant::now() >= deadline {
                return BeamPass::OutOfTime;
            }
            if layer.len() >= closure_cap {
                continue;
            }
            for_each_successor(problem, &state, |next, step, step_cost| {
                if step_cost == 0 && seen.insert(next) {
                    nodes.push(Node { state: next, parent: index, step });
                    layer.push((nodes.len() - 1) as u32);
                }
            });
        }
        ranked.clear();
        for &index in &layer {
            let state = &nodes[index as usize].state;
            if let Some(estimate) = problem.estimate(state)
                && moves + estimate < move_limit
            {
                ranked.push((estimate, Reverse(state.done.count_ones()), index));
            }
        }
        if ranked.len() > width {
            ranked.select_nth_unstable(width);
            ranked.truncate(width);
        }
        layer.clear();
        for &(_, _, index) in &ranked {
            let state = nodes[index as usize].state;
            expanded += 1;
            if expanded.is_multiple_of(DEADLINE_CHECK_INTERVAL) && Instant::now() >= deadline {
                return BeamPass::OutOfTime;
            }
            for_each_successor(problem, &state, |next, step, step_cost| {
                if step_cost == 1 && seen.insert(next) {
                    nodes.push(Node { state: next, parent: index, step });
                    layer.push((nodes.len() - 1) as u32);
                }
            });
        }
        if layer.is_empty() {
            return BeamPass::NotFound;
        }
    }
    BeamPass::NotFound
}

/// Builds the problem of running exactly `ops` (in some order) from the `start` stack to the
/// `goal` stack. `None` if the problem has more values or operations than the state can encode.
fn build_problem(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    surplus_dups: bool,
    ops: Vec<OpNodeId>,
    start: &[ValueNodeId],
    goal: &[ValueNodeId],
) -> Option<(Problem, State)> {
    if ops.len() > MAX_MASK_OPS as usize || start.len() > MAX_STACK {
        return None;
    }
    let mut local = vec![u8::MAX; graph.total_values() as usize];
    let mut values = 0usize;
    let mut number = |value: ValueNodeId| -> Option<u8> {
        if local[value.idx()] == u8::MAX {
            if values >= MAX_VALUES as usize {
                return None;
            }
            local[value.idx()] = values as u8;
            values += 1;
        }
        Some(local[value.idx()])
    };
    let mut start_state = State { done: 0, len: start.len() as u8, stack: [0; MAX_STACK] };
    for (slot, &value) in start_state.stack.iter_mut().zip(start) {
        *slot = number(value)?;
    }
    let mut predecessors = Vec::new();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut flippable = Vec::new();
    for &op in &ops {
        let view = graph.get_op(op);
        predecessors.push(view.predecessors.iter().fold(0u64, |mask, pred| {
            ops.iter().position(|&other| other == pred).map_or(mask, |bit| mask | 1 << bit)
        }));
        inputs.push(view.inputs_fifo.iter().map(|&value| number(value)).collect::<Option<_>>()?);
        outputs.push(view.outputs_fifo.iter().map(|&value| number(value)).collect::<Option<_>>()?);
        flippable.push(matches!(view.kind, OpNodeKind::Flippable(_)));
    }
    let goal_stack: Vec<u8> = goal.iter().map(|&value| number(value)).collect::<Option<_>>()?;
    let mut readers = vec![Vec::<(u64, u32)>::new(); values];
    let mut producer_bit = vec![0u64; values];
    for (index, (op_inputs, op_outputs)) in inputs.iter().zip(&outputs).enumerate() {
        let bit = 1u64 << index;
        for &value in op_inputs {
            let entry: &mut Vec<(u64, u32)> = &mut readers[usize::from(value)];
            match entry.iter_mut().find(|(reader, _)| *reader == bit) {
                Some((_, slots)) => *slots += 1,
                None => entry.push((bit, 1)),
            }
        }
        for &value in op_outputs {
            producer_bit[usize::from(value)] = bit;
        }
    }
    let mut output_uses = vec![0u32; values];
    for &value in &goal_stack {
        output_uses[usize::from(value)] += 1;
    }
    let goal_done = if ops.len() == 64 { u64::MAX } else { (1u64 << ops.len()) - 1 };
    let goal_height = goal_heights(&goal_stack, &output_uses);
    let goal_position = goal_positions(&goal_stack, values);
    let problem = Problem {
        ops,
        finalization,
        config,
        readers,
        output_uses,
        producer_bit,
        predecessors,
        inputs,
        outputs,
        flippable,
        goal_done,
        goal_stack,
        goal_height,
        goal_position,
        surplus_dups,
    };
    Some((problem, start_state))
}

fn goal_heights(goal_stack: &[u8], output_uses: &[u32]) -> Vec<u8> {
    let mut goal_height = vec![u8::MAX; output_uses.len()];
    for (position, &value) in goal_stack.iter().enumerate() {
        if output_uses[usize::from(value)] == 1 {
            goal_height[usize::from(value)] = (goal_stack.len() - 1 - position) as u8;
        }
    }
    goal_height
}

fn goal_positions(goal_stack: &[u8], values: usize) -> Option<Vec<u8>> {
    let mut goal_position = vec![u8::MAX; values];
    for (position, &value) in goal_stack.iter().enumerate() {
        if goal_position[usize::from(value)] != u8::MAX {
            return None;
        }
        goal_position[usize::from(value)] = position as u8;
    }
    Some(goal_position)
}

fn for_each_successor(problem: &Problem, state: &State, mut visit: impl FnMut(State, Move, u32)) {
    let fifo = state.fifo();
    let len = fifo.len();
    for (index, &op) in problem.ops.iter().enumerate() {
        let bit = 1u64 << index;
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
    let start = input_stack(graph);
    replay_from(graph, next_alloc_id, start.fifo(), nodes, goal)
}

fn replay_from(
    graph: &OpGraph,
    next_alloc_id: StaticAllocId,
    start: &[ValueNodeId],
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
    let mut stack =
        TrackedStack::new_from_parts(next_alloc_id, |op| ops.push(op), start, Vec::new());
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

/// Window sizes, in operations, swept by [`reoptimize_windows`] from small to large.
const WINDOW_OPS: std::ops::RangeInclusive<usize> = 2..=10;

struct Boundary {
    /// Index into the schedule right after the boundary.
    position: usize,
    stack: EvmStack,
    /// The operation the boundary follows, `None` for the start, spills and a trailing shuffle.
    op: Option<OpNodeId>,
}

/// Local exact re-optimization of a finished schedule: the stack states right after
/// each operation are fixed points, and the moves and operations between two points a few
/// operations apart (sizes from [`WINDOW_OPS`]) are re-solved by the A* with the end state as
/// goal. A window is replaced whenever the A* finds strictly fewer moves; returns `None` if
/// nothing improved before `deadline`.
pub fn reoptimize_windows(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    next_alloc_id: StaticAllocId,
    schedule: &[StackOps],
    deadline: Instant,
) -> Option<Vec<StackOps>> {
    if !schedule.iter().all(|op| {
        matches!(
            op,
            StackOps::Swap(_)
                | StackOps::Dup(_)
                | StackOps::Pop
                | StackOps::Op(_)
                | StackOps::Flipped(_)
                | StackOps::CallRetPush(_)
                | StackOps::Store(_)
                | StackOps::Load(_)
        )
    }) {
        return None;
    }
    let mut schedule = schedule.to_vec();
    let mut improved_any = false;
    for window_ops in WINDOW_OPS {
        if Instant::now() >= deadline {
            break;
        }
        improved_any |= sweep_windows(
            graph,
            finalization,
            config,
            next_alloc_id,
            &mut schedule,
            window_ops,
            deadline,
        );
    }
    improved_any.then_some(schedule)
}

/// Re-solves every window of `window_ops` operations until a sweep finds no improvement.
#[allow(clippy::too_many_arguments)]
fn sweep_windows(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    next_alloc_id: StaticAllocId,
    schedule: &mut Vec<StackOps>,
    window_ops: usize,
    deadline: Instant,
) -> bool {
    let op_of = |operation: StackOps| {
        let (index, ret_dest_push) = match operation {
            StackOps::Op(index) | StackOps::Flipped(index) => (index, false),
            StackOps::CallRetPush(index) => (index, true),
            _ => return None,
        };
        graph.op_ids().find(|&op| match graph.get_op(op).kind {
            OpNodeKind::Flippable(idx) | OpNodeKind::Normal(idx) => !ret_dest_push && idx == index,
            OpNodeKind::RetDestPush(idx) => ret_dest_push && idx == index,
        })
    };
    let mut improved_any = false;
    let mut first = 0;
    loop {
        let mut boundaries = vec![Boundary { position: 0, stack: input_stack(graph), op: None }];
        let mut stack = boundaries[0].stack.clone();
        let mut spilled = Vec::new();
        for (position, &operation) in schedule.iter().enumerate() {
            match operation {
                StackOps::Swap(depth) => stack.swap_with_top(depth.into()),
                StackOps::Dup(depth) => stack.duplicate(depth.into()),
                StackOps::Pop => {
                    stack.pop().expect("pop on empty stack");
                }
                // Spills stay where they are: they are fixed boundaries no window crosses.
                StackOps::Store(_) | StackOps::Load(_) => {
                    if let StackOps::Load(alloc) = operation {
                        stack.push(spilled[(alloc - next_alloc_id) as usize]);
                    } else {
                        spilled.push(stack.pop().expect("store on empty stack"));
                    }
                    boundaries.push(Boundary {
                        position: position + 1,
                        stack: stack.clone(),
                        op: None,
                    });
                }
                _ => {
                    let op = op_of(operation).expect("scheduled operation missing from graph");
                    let view = graph.get_op(op);
                    for _ in view.inputs_fifo {
                        stack.pop().expect("missing input");
                    }
                    for &output in view.outputs_fifo.iter().rev() {
                        stack.push(output);
                    }
                    boundaries.push(Boundary {
                        position: position + 1,
                        stack: stack.clone(),
                        op: Some(op),
                    });
                }
            }
        }
        if boundaries.last().is_some_and(|last| last.position < schedule.len()) {
            boundaries.push(Boundary { position: schedule.len(), stack: stack.clone(), op: None });
        }

        let mut improvement = None;
        for start_index in first..boundaries.len().saturating_sub(1) {
            if Instant::now() >= deadline {
                return improved_any;
            }
            let end_index = (start_index + window_ops).min(boundaries.len() - 1);
            let (from, to) = (&boundaries[start_index], &boundaries[end_index]);
            let window = &schedule[from.position..to.position];
            let moves = window.iter().filter(|op| op.gas_cost(config) > 0).count() as u32;
            if moves == 0
                || window.iter().any(|op| matches!(op, StackOps::Store(_) | StackOps::Load(_)))
                || from.stack.fifo().len() > MAX_STACK
                || to.stack.fifo().len() > MAX_STACK
            {
                continue;
            }
            let terminates = end_index == boundaries.len() - 1
                && to.position == schedule.len()
                && finalization == BlockFinalization::LastOpTerminates;
            let ops = boundaries[start_index + 1..=end_index]
                .iter()
                .filter_map(|boundary| boundary.op)
                .collect();
            let (window_finalization, goal) = if terminates {
                (BlockFinalization::LastOpTerminates, &[][..])
            } else {
                (BlockFinalization::ShuffleToOutputs, to.stack.fifo())
            };
            let Some((problem, state)) = build_problem(
                graph,
                window_finalization,
                config,
                true,
                ops,
                from.stack.fifo(),
                goal,
            ) else {
                continue;
            };
            if let Some((nodes, goal)) = astar(&problem, state, moves, deadline) {
                let replacement =
                    replay_from(graph, next_alloc_id, from.stack.fifo(), &nodes, goal);
                improvement = Some((from.position, to.position, replacement, start_index));
                break;
            }
        }
        let Some((from, to, replacement, start_index)) = improvement else {
            return improved_any;
        };
        schedule.splice(from..to, replacement);
        improved_any = true;
        first = start_index.saturating_sub(window_ops);
    }
}

fn input_stack(graph: &OpGraph) -> EvmStack {
    let mut stack = EvmStack::new();
    for input in graph.input_values_fifo().iter().rev() {
        stack.push(input);
    }
    stack
}
