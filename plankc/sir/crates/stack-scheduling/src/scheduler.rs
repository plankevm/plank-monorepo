use std::cell::Cell;

use crate::{
    BlockFinalization,
    exact::{exact_schedule, reoptimize_windows},
    greedy_intra_op_scheduler::greedy_schedule_op,
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpNodeId, OpNodeKind, OpSet, OpSetMut, ValueNodeId},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack},
    treegraph::build_tree_graph,
    validation,
};
use plank_core::Idx;
use sir_data::StaticAllocId;
use smallvec::SmallVec;

const SCRATCH_OP_SET_INLINE_CAPACITY: usize = 512 / BitsetWord::BITS as usize;
/// Work units granted per thousand units of effort for a single graph. A work unit is one expanded
/// A* state, one window tried, or one greedy step of a policy candidate. Counting work
/// instead of time makes the schedule a function of the graph and the effort only. Tuned so that a
/// validation set of ~1,500 graphs takes under a second at effort 1000 on an M-series laptop.
const WORK_PER_THOUSAND_EFFORT: u64 = 1_400;
/// Graphs with more than `BUDGET_SCALE_OPS` operations get a budget proportional to their
/// operation count, up to `MAX_BUDGET_SCALE` times the base budget. Large graphs are few, always
/// use their whole budget and keep improving with more work, while small graphs mostly finish
/// early.
const BUDGET_SCALE_OPS: u64 = 32;
const MAX_BUDGET_SCALE: u64 = 8;
/// Share of the search budget the exact A* may use before falling back to the heuristic search.
const EXACT_BUDGET_SHARE: f64 = 0.5;
/// Share of the whole budget reserved for [`reoptimize_windows`] on the final schedule.
const WINDOW_BUDGET_SHARE: f64 = 0.25;
/// Stops the search once this many consecutive candidates failed to improve, scaled by graph
/// size, so small graphs whose neighbourhood is exhausted do not burn their whole budget.
const STAGNATION_PER_OP: u32 = 400;
const STAGNATION_MIN: u32 = 2_000;
/// Graphs with fewer orderable operations plus flip choices are exhausted by plain hill climbing.
const MIN_CHOICES_FOR_RESTARTS: usize = 4;
/// A restart applies `RESTART_KICK_BASE + ops / RESTART_KICK_OPS_PER_MUTATION` random mutations
/// to the best policy found so far.
const RESTART_KICK_BASE: usize = 3;
const RESTART_KICK_OPS_PER_MUTATION: usize = 8;
/// Blocks with fewer inputs rarely need spills, so the search only varies [`Policy::pre_spill`]
/// over their leading dead inputs.
const PRE_SPILL_MIN_INPUTS: u32 = 14;
const MAX_PRE_SPILL: u8 = 8;
const PRE_SWAPS: usize = 3;
/// Number of independent policy searches, each with its own random seed and an equal share of
/// the search budget.
const SEARCHES: u64 = 2;
/// Graphs with at most this many operations only mutate op swaps up to the deepest stack seen.
const OP_SWAP_DEPTH_CAP_MAX_OPS: usize = 32;

pub struct SearchResult {
    pub ops: Box<[StackOps]>,
    pub spill_count: u32,
}

/// How a flippable operation chooses its operand order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlipChoice {
    /// Pick the orientation whose operand preparation is cheaper right now.
    Cheapest,
    Straight,
    Flipped,
}

/// Genome steering [`greedy_schedule`]: among ready operations the one with the smallest
/// priority is scheduled next.
#[derive(Debug, Clone)]
pub struct Policy {
    pub priority: Vec<u32>,
    pub flips: Vec<FlipChoice>,
    /// Permute the block inputs into [`pre_shuffle_layout`] before scheduling any operation.
    pub pre_shuffle: bool,
    /// Number of block inputs spilled (or popped, if dead) off the top of the stack before
    /// anything else. Deep blocks often pass their top inputs through to deep output slots;
    /// storing them at once and loading them at the end beats dragging them along past
    /// `dup`/`swap` reach. Dead inputs on top otherwise get buried under operation results.
    pub pre_spill: u8,
    /// Swaps applied right after the pre-spills (depth 0 = none). They let a pass-through input
    /// sink towards its output slot while it is still cheap to reach.
    pub pre_swaps: [u8; PRE_SWAPS],
    /// Per operation: up to two swaps applied right before preparing its operands (0 = none), to
    /// park values at the depth they are wanted later.
    pub op_swaps: Vec<[u8; 2]>,
    /// Permute the stack into [`mid_shuffle_layout`] right before preparing the operands of this
    /// operation. Some blocks are cheapest when a long permutation runs once an op or two has
    /// shrunk the stack, which neither the up-front shuffle nor the op swaps reach.
    pub shuffle_before: Option<OpNodeId>,
}

impl Policy {
    fn graph_order(graph: &OpGraph) -> Self {
        let n = graph.total_ops() as usize;
        Self {
            priority: (0..n as u32).map(|i| i * PRIORITY_SPACING).collect(),
            flips: vec![FlipChoice::Cheapest; n],
            pre_shuffle: false,
            pre_spill: 0,
            pre_swaps: [0; PRE_SWAPS],
            op_swaps: vec![[0; 2]; n],
            shuffle_before: None,
        }
    }
}

const PRIORITY_SPACING: u32 = 1 << 16;

pub fn schedule(
    finalization: BlockFinalization,
    next_alloc_id: StaticAllocId,
    shuffle: ShuffleConfig,
    graph: &OpGraph,
) -> SearchResult {
    schedule_with_effort(finalization, next_alloc_id, shuffle, graph, 0)
}

pub fn schedule_with_effort(
    finalization: BlockFinalization,
    next_alloc_id: StaticAllocId,
    shuffle: ShuffleConfig,
    graph: &OpGraph,
    effort: u32,
) -> SearchResult {
    let mut work = u64::from(effort)
        * WORK_PER_THOUSAND_EFFORT
        * u64::from(graph.total_ops()).clamp(BUDGET_SCALE_OPS, BUDGET_SCALE_OPS * MAX_BUDGET_SCALE)
        / BUDGET_SCALE_OPS
        / 1000;
    let window_work = (work as f64 * WINDOW_BUDGET_SHARE) as u64;
    work -= window_work;
    let exact_result = |ops: Vec<StackOps>| {
        validation::validate(graph, finalization, shuffle, next_alloc_id, next_alloc_id, &ops)
            .expect("exact scheduler produced an invalid schedule");
        SearchResult { ops: ops.into_boxed_slice(), spill_count: 0 }
    };
    if effort > 0 {
        let mut exact_work = (work as f64 * EXACT_BUDGET_SHARE) as u64;
        let exact_budget = exact_work;
        if let Some(ops) =
            exact_schedule(graph, finalization, shuffle, next_alloc_id, &mut exact_work)
        {
            return exact_result(ops);
        }
        work = (work - (exact_budget - exact_work)).max(1);
    }
    let trees = build_tree_graph(graph);
    let tree_dag = Dag::new(&trees.graph);
    let search_work = work.div_ceil(SEARCHES);
    let search = |seed: u64| {
        let policy = search_policy(
            finalization,
            shuffle,
            &trees.graph,
            &tree_dag,
            search_work,
            Policy::graph_order(&trees.graph),
            seed,
        );
        let mut ops = Vec::new();
        let new_next_alloc_id = greedy_schedule(
            |op| ops.push(op),
            finalization,
            next_alloc_id,
            shuffle,
            &trees.graph,
            &tree_dag,
            &policy,
            None,
            effort > 0,
        )
        .expect("unbounded schedule cannot be cut off");
        (trees.expand_schedule(graph, &ops), new_next_alloc_id)
    };
    // Independent policy searches with different random streams explore different basins.
    let searches = if effort > 0 { SEARCHES } else { 1 };
    let mut searched: Vec<_> = (0..searches).map(search).collect();
    // Stable sort: equal-cost results stay in search order.
    searched.sort_by_key(|(ops, _)| crate::stack::gas_cost(ops, shuffle));
    searched.dedup_by(|(a, _), (b, _)| a == b);
    let finish_work = window_work / searched.len() as u64;
    // Every distinct search result is window-optimized: a slightly worse search
    // result sometimes ends cheaper after the windows.
    let finish = |(ops, new_next_alloc_id): (Box<[StackOps]>, StaticAllocId)| {
        if effort == 0 {
            return (ops, new_next_alloc_id);
        }
        let mut work = finish_work;
        let ops = reoptimize_windows(graph, finalization, shuffle, next_alloc_id, &ops, &mut work)
            .map_or(ops, Vec::into_boxed_slice);
        (ops, new_next_alloc_id)
    };
    // `min_by_key` keeps the first of equal-cost results.
    let (ops, new_next_alloc_id) = searched
        .into_iter()
        .map(finish)
        .min_by_key(|(ops, _)| crate::stack::gas_cost(ops, shuffle))
        .expect("at least one search ran");
    let spill_count = new_next_alloc_id - next_alloc_id;

    validation::validate(graph, finalization, shuffle, next_alloc_id, new_next_alloc_id, &ops)
        .expect("stack scheduler produced an invalid schedule");

    SearchResult { ops, spill_count }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Hill climbing over [`Policy`] genomes: mutate the best genome, rerun the greedy scheduler and
/// keep the mutant if it is no more expensive. Equal-cost mutants are accepted to drift across
/// plateaus.
fn search_policy(
    finalization: BlockFinalization,
    config: ShuffleConfig,
    graph: &OpGraph,
    dag: &Dag,
    mut work: u64,
    mut best: Policy,
    seed: u64,
) -> Policy {
    let n = graph.total_ops() as usize;
    if work == 0 || n == 0 {
        return best;
    }
    let flippable: Vec<usize> = graph
        .op_ids()
        .filter(|&op| matches!(graph.get_op(op).kind, OpNodeKind::Flippable(_)))
        .map(|op| op.idx())
        .collect();

    let mut trace = Trace::default();
    let mut recorded = Vec::new();
    let mut best_gas =
        policy_gas_resumed(finalization, config, graph, dag, &best, None, None, &mut trace.steps)
            .expect("unbounded")
            .0;
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15
        ^ (n as u64).wrapping_mul(0x2545_F491_4F6C_DD1D)
        ^ seed.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    let stagnation_limit = STAGNATION_MIN.max(STAGNATION_PER_OP * n as u32);
    let max_pre_spill = if graph.input_values_fifo().len() >= PRE_SPILL_MIN_INPUTS {
        MAX_PRE_SPILL.min(graph.input_values_fifo().len() as u8)
    } else {
        graph.input_values_fifo().iter().take_while(|&value| is_dead(graph, value)).count() as u8
    };
    let max_pre_swap_depth = (graph.input_values_fifo().len().saturating_sub(1) as u8).min(16);
    // Swaps deeper than any stack the search has seen only waste candidates on small graphs.
    // Larger graphs search better with the full range: such genes are latent, not wasted.
    let cap_op_swap_depth = n <= OP_SWAP_DEPTH_CAP_MAX_OPS;
    let mut max_op_swap_depth = if cap_op_swap_depth { deepest_swap(&trace.steps) } else { 16 };
    let restarts_allowed = n + flippable.len() >= MIN_CHOICES_FOR_RESTARTS;
    let mut stagnant = 0u32;
    let mut current = best.clone();
    let mut current_gas = best_gas;
    let mut candidate = best.clone();

    while best_gas > 0 && work > 0 {
        if stagnant >= stagnation_limit {
            if !restarts_allowed {
                break;
            }
            current.clone_from(&best);
            for _ in 0..RESTART_KICK_BASE + n / RESTART_KICK_OPS_PER_MUTATION {
                mutate(
                    &mut current,
                    &flippable,
                    max_pre_spill,
                    max_pre_swap_depth,
                    max_op_swap_depth,
                    &mut rng,
                );
            }
            current_gas = policy_gas_resumed(
                finalization,
                config,
                graph,
                dag,
                &current,
                None,
                None,
                &mut trace.steps,
            )
            .expect("unbounded")
            .0;
            work = work.saturating_sub(trace.steps.len() as u64 + 1);
            if cap_op_swap_depth {
                max_op_swap_depth = max_op_swap_depth.max(deepest_swap(&trace.steps));
            }
            stagnant = 0;
        }
        candidate.priority.clone_from(&current.priority);
        candidate.flips.clone_from(&current.flips);
        candidate.pre_shuffle = current.pre_shuffle;
        candidate.pre_spill = current.pre_spill;
        candidate.pre_swaps = current.pre_swaps;
        candidate.op_swaps.clone_from(&current.op_swaps);
        candidate.shuffle_before = current.shuffle_before;
        let mutations = 1 + rng.below(3).saturating_sub(1);
        for _ in 0..mutations {
            mutate(
                &mut candidate,
                &flippable,
                max_pre_spill,
                max_pre_swap_depth,
                max_op_swap_depth,
                &mut rng,
            );
        }
        stagnant += 1;
        let evaluated = policy_gas_resumed(
            finalization,
            config,
            graph,
            dag,
            &candidate,
            Some((&current, &trace, current_gas)),
            Some(current_gas),
            &mut recorded,
        );
        work = work.saturating_sub(recorded.len() as u64 + 1);
        if let Some((gas, divergence)) = evaluated {
            if cap_op_swap_depth {
                max_op_swap_depth = max_op_swap_depth.max(deepest_swap(&recorded));
            }
            trace.steps.truncate(divergence);
            trace.steps.append(&mut recorded);
            if gas < current_gas {
                stagnant = 0;
            }
            current_gas = gas;
            std::mem::swap(&mut current, &mut candidate);
            if current_gas < best_gas {
                best_gas = current_gas;
                best.clone_from(&current);
            }
        }
    }

    best
}

fn mutate(
    policy: &mut Policy,
    flippable: &[usize],
    max_pre_spill: u8,
    max_pre_swap_depth: u8,
    max_op_swap_depth: u8,
    rng: &mut Rng,
) {
    let n = policy.priority.len();
    if rng.below(16) == 0 {
        policy.pre_shuffle = !policy.pre_shuffle;
        return;
    }
    if max_pre_swap_depth > 0 && rng.below(16) == 0 {
        let swap = &mut policy.pre_swaps[rng.below(PRE_SWAPS)];
        *swap = if *swap != 0 && rng.below(2) == 0 {
            0
        } else {
            1 + rng.below(usize::from(max_pre_swap_depth)) as u8
        };
        return;
    }
    if max_pre_spill > 0 && rng.below(16) == 0 {
        policy.pre_spill =
            if policy.pre_spill == 0 || (policy.pre_spill < max_pre_spill && rng.below(2) == 0) {
                policy.pre_spill + 1
            } else {
                policy.pre_spill - 1
            };
        return;
    }
    if rng.below(16) == 0 {
        policy.shuffle_before = if policy.shuffle_before.is_some() && rng.below(2) == 0 {
            None
        } else {
            Some(OpNodeId::new(rng.below(n) as u32))
        };
        return;
    }
    if rng.below(2) == 0 {
        let swap = &mut policy.op_swaps[rng.below(n)][rng.below(2)];
        *swap = if *swap != 0 && rng.below(2) == 0 {
            0
        } else {
            1 + rng.below(usize::from(max_op_swap_depth)) as u8
        };
        return;
    }
    if !flippable.is_empty() && (n == 1 || rng.below(4) == 0) {
        let op = flippable[rng.below(flippable.len())];
        policy.flips[op] = match (policy.flips[op], rng.below(2)) {
            (FlipChoice::Cheapest, 0) | (FlipChoice::Flipped, _) => FlipChoice::Straight,
            (FlipChoice::Cheapest, _) | (FlipChoice::Straight, 0) => FlipChoice::Flipped,
            (FlipChoice::Straight, _) => FlipChoice::Cheapest,
        };
        return;
    }
    let a = rng.below(n);
    match rng.below(3) {
        0 => {
            let b = rng.below(n);
            policy.priority.swap(a, b);
        }
        1 => {
            let b = rng.below(n);
            let below = policy.priority[b].wrapping_sub(1 + rng.below(16) as u32);
            policy.priority[a] = below;
        }
        _ => {
            policy.priority[a] = rng.next() as u32;
        }
    }
}

/// Dependency edges of a graph in the form [`greedy_schedule`] needs to keep its set of ready
/// operations up to date in time proportional to the edges instead of rescanning all operations
/// after every step.
pub struct Dag {
    successors: Vec<Vec<OpNodeId>>,
    predecessor_counts: Vec<u32>,
}

impl Dag {
    pub fn new(graph: &OpGraph) -> Self {
        let successors = graph.immediate_successors();
        let mut predecessor_counts = vec![0; successors.len()];
        for &successor in successors.iter().flatten() {
            predecessor_counts[successor.idx()] += 1;
        }
        Self { successors, predecessor_counts }
    }
}

/// Schedules every operation greedily in the order given by `policy`. With a `bound`, gives up
/// and returns `None` as soon as the accumulated gas exceeds it. `thorough_final_shuffle` also
/// tries the scrambled final shuffle strategies, which never cost more but are too slow to run
/// for every search candidate.
pub fn greedy_schedule(
    ops_sink: impl FnMut(StackOps),
    finalization: BlockFinalization,
    next_alloc_id: StaticAllocId,
    config: ShuffleConfig,
    graph: &OpGraph,
    dag: &Dag,
    policy: &Policy,
    bound: Option<(&Cell<u64>, u64)>,
    thorough_final_shuffle: bool,
) -> Option<StaticAllocId> {
    let exceeded = || bound.is_some_and(|(gas, bound)| gas.get() > bound);
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
    schedule_prelude(config, &mut stack, graph, policy);

    let mut waiting_on: SmallVec<[u32; 64]> = SmallVec::from_slice(&dag.predecessor_counts);
    let mut ready: SmallVec<[OpNodeId; 32]> =
        graph.op_ids().filter(|op| waiting_on[op.idx()] == 0).collect();
    // Cheapest-next-op selection is myopic: on the corpus it is 4-6% worse than
    // taking ready ops in graph order, which tracks the tree graph's operand order.
    while let Some((position, &op)) =
        ready.iter().enumerate().min_by_key(|(_, op)| (policy.priority[op.idx()], op.idx()))
    {
        ready.swap_remove(position);
        for &successor in &dag.successors[op.idx()] {
            waiting_on[successor.idx()] -= 1;
            if waiting_on[successor.idx()] == 0 {
                ready.push(successor);
            }
        }
        schedule_step(config, &mut stack, graph, policy, op, complete.as_ref());
        complete.add(op);
        if exceeded() {
            return None;
        }
    }

    if finalization == BlockFinalization::ShuffleToOutputs {
        greedy_shuffler::shuffle_to(
            config,
            &mut stack,
            graph.output_values_fifo(),
            thorough_final_shuffle || graph.total_ops() == 0,
        );
    }
    if exceeded() {
        return None;
    }

    Some(stack.into_next_alloc_id())
}

fn schedule_prelude<Sink: FnMut(StackOps)>(
    config: ShuffleConfig,
    stack: &mut TrackedStack<Sink>,
    graph: &OpGraph,
    policy: &Policy,
) {
    for _ in 0..policy.pre_spill {
        if is_dead(graph, stack.top().expect("pre-spilled inputs are on the stack")) {
            stack.pop();
        } else {
            stack.spill_top();
        }
    }
    for depth in policy.pre_swaps {
        if depth != 0 && usize::from(depth) < stack.len() as usize {
            stack.swap(depth);
        }
    }
    if policy.pre_shuffle {
        greedy_shuffler::shuffle_to(config, stack, &pre_shuffle_layout(graph, policy), false);
    }
}

fn schedule_step<Sink: FnMut(StackOps)>(
    config: ShuffleConfig,
    stack: &mut TrackedStack<Sink>,
    graph: &OpGraph,
    policy: &Policy,
    op: OpNodeId,
    complete: OpSet<'_>,
) {
    if policy.shuffle_before == Some(op) {
        let layout = mid_shuffle_layout(graph, policy, stack.fifo(), complete);
        greedy_shuffler::shuffle_to(config, stack, &layout, false);
    }
    for swap in policy.op_swaps[op.idx()] {
        if swap != 0 && usize::from(swap) < stack.len() as usize {
            stack.swap(swap);
        }
    }
    let flipped = match (graph.get_op(op).kind, policy.flips[op.idx()]) {
        (OpNodeKind::Flippable(_), FlipChoice::Flipped) => true,
        (OpNodeKind::Flippable(_), FlipChoice::Cheapest) => {
            let trial = |flipped| {
                let mut ops = Vec::new();
                let mut trial_stack = stack.clone_with(|op| ops.push(op));
                greedy_schedule_op(config, &mut trial_stack, graph, op, complete, flipped);
                drop(trial_stack);
                ops
            };
            let flipped_ops = trial(true);
            let straight_ops = trial(false);
            // The operand preparer is deterministic, so replaying the cheaper trial equals
            // preparing that orientation again.
            let ops = if crate::stack::gas_cost(&flipped_ops, config)
                < crate::stack::gas_cost(&straight_ops, config)
            {
                flipped_ops
            } else {
                straight_ops
            };
            for stack_op in ops {
                match stack_op {
                    StackOps::Pop => stack.pop(),
                    StackOps::Swap(depth) => stack.swap(depth),
                    StackOps::Dup(depth) => stack.dup(depth),
                    StackOps::Store(expected) => assert_eq!(stack.spill_top(), expected),
                    StackOps::Load(allocation) => stack.load(allocation),
                    StackOps::Op(_) => stack.op(graph, op, false),
                    StackOps::Flipped(_) => stack.op(graph, op, true),
                    StackOps::Exchange(_, _) | StackOps::CallRetPush(_) => {
                        unreachable!("operand preparer produced an unsupported operation")
                    }
                }
            }
            return;
        }
        _ => false,
    };
    greedy_schedule_op(config, stack, graph, op, complete, flipped);
}

/// Deepest swap any of `steps` could perform, at least 1.
fn deepest_swap(steps: &[Checkpoint]) -> u8 {
    let deepest = steps.iter().map(|step| step.stack.len()).max().unwrap_or(0);
    deepest.saturating_sub(1).clamp(1, 16) as u8
}

/// Greedy state right before one step of a [`Trace`].
struct Checkpoint {
    op: OpNodeId,
    gas: u64,
    stack: SmallVec<[ValueNodeId; 32]>,
    spilled: Vec<ValueNodeId>,
}

/// The steps of the greedy schedule of the policy the search currently holds. A mutant schedules
/// exactly like it up to the first step where it picks a different op or treats the op
/// differently, so its evaluation resumes from the checkpoint there instead of starting over.
#[derive(Default)]
struct Trace {
    steps: Vec<Checkpoint>,
}

/// [`policy_gas`] that reuses the schedule prefix `candidate` shares with `reference`, whose steps
/// are `trace` and whose gas is the last element. Records the candidate's checkpoints from the
/// divergence on into `recorded` and returns the gas and the divergence step.
fn policy_gas_resumed(
    finalization: BlockFinalization,
    config: ShuffleConfig,
    graph: &OpGraph,
    dag: &Dag,
    candidate: &Policy,
    reference: Option<(&Policy, &Trace, u64)>,
    bound: Option<u64>,
    recorded: &mut Vec<Checkpoint>,
) -> Option<(u64, usize)> {
    recorded.clear();
    let mut complete_backing = SmallVec::<[BitsetWord; SCRATCH_OP_SET_INLINE_CAPACITY]>::new();
    complete_backing.resize(graph.words_per_set() as usize, 0);
    let mut complete = OpSetMut::new(&mut complete_backing, graph.total_ops());
    let mut waiting_on: SmallVec<[u32; 64]> = SmallVec::from_slice(&dag.predecessor_counts);
    let mut ready: SmallVec<[OpNodeId; 32]> =
        graph.op_ids().filter(|op| waiting_on[op.idx()] == 0).collect();
    let pick = |ready: &mut SmallVec<[OpNodeId; 32]>, waiting_on: &mut SmallVec<[u32; 64]>| {
        let (position, &op) = ready
            .iter()
            .enumerate()
            .min_by_key(|(_, op)| (candidate.priority[op.idx()], op.idx()))?;
        ready.swap_remove(position);
        for &successor in &dag.successors[op.idx()] {
            waiting_on[successor.idx()] -= 1;
            if waiting_on[successor.idx()] == 0 {
                ready.push(successor);
            }
        }
        Some(op)
    };

    let same_prelude = reference.is_some_and(|(reference, _, _)| {
        reference.pre_spill == candidate.pre_spill
            && reference.pre_swaps == candidate.pre_swaps
            && reference.pre_shuffle == candidate.pre_shuffle
            && (!candidate.pre_shuffle || reference.priority == candidate.priority)
    });
    let mut divergence = 0;
    let mut next = pick(&mut ready, &mut waiting_on);
    if let Some((reference, trace, reference_gas)) = reference.filter(|_| same_prelude) {
        while let Some(op) = next {
            let step = &trace.steps[divergence];
            if op != step.op
                || reference.op_swaps[op.idx()] != candidate.op_swaps[op.idx()]
                || reference.flips[op.idx()] != candidate.flips[op.idx()]
                || (reference.shuffle_before == Some(op)) != (candidate.shuffle_before == Some(op))
                || (candidate.shuffle_before == Some(op)
                    && reference.priority != candidate.priority)
            {
                break;
            }
            complete.add(op);
            divergence += 1;
            next = pick(&mut ready, &mut waiting_on);
        }
        if next.is_none() {
            return Some((reference_gas, divergence));
        }
    }

    let gas = Cell::new(0u64);
    let sink = |op: StackOps| gas.set(gas.get() + u64::from(op.gas_cost(config)));
    let mut stack = match reference {
        Some((_, trace, _)) if same_prelude => {
            let step = &trace.steps[divergence];
            gas.set(step.gas);
            TrackedStack::new_from_parts(
                StaticAllocId::ZERO,
                sink,
                &step.stack,
                step.spilled.clone(),
            )
        }
        _ => {
            let mut inner = EvmStack::new();
            for input in graph.input_values_fifo().iter().rev() {
                inner.push(input);
            }
            let mut stack = TrackedStack::new_from_evm(StaticAllocId::ZERO, sink, inner, 8);
            schedule_prelude(config, &mut stack, graph, candidate);
            stack
        }
    };
    let exceeded = || bound.is_some_and(|bound| gas.get() > bound);
    while let Some(op) = next {
        recorded.push(Checkpoint {
            op,
            gas: gas.get(),
            stack: SmallVec::from_slice(stack.fifo()),
            spilled: stack.underlying_spilled().to_vec(),
        });
        schedule_step(config, &mut stack, graph, candidate, op, complete.as_ref());
        complete.add(op);
        if exceeded() {
            return None;
        }
        next = pick(&mut ready, &mut waiting_on);
    }
    if finalization == BlockFinalization::ShuffleToOutputs {
        greedy_shuffler::shuffle_to(
            config,
            &mut stack,
            graph.output_values_fifo(),
            graph.total_ops() == 0,
        );
    }
    drop(stack);
    if exceeded() {
        return None;
    }
    Some((gas.get(), divergence))
}

fn is_dead(graph: &OpGraph, value: ValueNodeId) -> bool {
    graph.get_consumers(value).count_members() == 0 && !graph.output_values_fifo().contains(&value)
}

/// Input layout that lets operations consume their inputs straight off the top: inputs feeding
/// operations come first in the order the policy consumes them, followed by the inputs passed
/// through to the block outputs, already in output order. Doing this permutation once up front is
/// often cheaper than fixing up pass-through values at the end with deep swaps.
fn pre_shuffle_layout(graph: &OpGraph, policy: &Policy) -> Vec<ValueNodeId> {
    let pre_spilled = |value: ValueNodeId| value.idx() < usize::from(policy.pre_spill);
    let mut passed_through = Vec::new();
    for &value in graph.output_values_fifo().iter().rev() {
        if graph.is_input(value) && !pre_spilled(value) && !passed_through.contains(&value) {
            passed_through.push(value);
        }
    }
    passed_through.reverse();

    let mut order: Vec<OpNodeId> = graph.op_ids().collect();
    order.sort_by_key(|op| (policy.priority[op.idx()], op.idx()));
    let mut layout = Vec::new();
    for op in order {
        for &value in graph.get_op(op).inputs_fifo {
            if graph.is_input(value)
                && !pre_spilled(value)
                && !layout.contains(&value)
                && !passed_through.contains(&value)
            {
                layout.push(value);
            }
        }
    }
    layout.extend(passed_through);
    layout
}

/// Stack layout for [`Policy::shuffle_before`]: values the remaining operations read come first in
/// the order the policy consumes them, followed by the block outputs already on the stack, in
/// output order. Values that are neither are dropped. Every value appears once, as the operand
/// preparer expects a stack without duplicates.
fn mid_shuffle_layout(
    graph: &OpGraph,
    policy: &Policy,
    stack: &[ValueNodeId],
    complete: OpSet<'_>,
) -> Vec<ValueNodeId> {
    let mut outputs = Vec::new();
    for &value in graph.output_values_fifo().iter().rev() {
        if stack.contains(&value) && !outputs.contains(&value) {
            outputs.push(value);
        }
    }
    outputs.reverse();
    let mut order: Vec<OpNodeId> = graph.op_ids().filter(|&op| !complete.contains(op)).collect();
    order.sort_by_key(|op| (policy.priority[op.idx()], op.idx()));
    let mut layout = Vec::new();
    for op in order {
        for &value in graph.get_op(op).inputs_fifo {
            if stack.contains(&value) && !layout.contains(&value) && !outputs.contains(&value) {
                layout.push(value);
            }
        }
    }
    layout.extend(outputs);
    layout
}
