use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use crate::{
    BlockFinalization,
    greedy_intra_op_scheduler::greedy_schedule_op,
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpNodeId, OpNodeKind, OpSetMut, ValueNodeId},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack},
    treegraph::build_tree_graph,
    validation,
};
use plank_core::Idx;
use sir_data::StaticAllocId;
use smallvec::SmallVec;

const SCRATCH_OP_SET_INLINE_CAPACITY: usize = 512 / BitsetWord::BITS as usize;
/// Search wall time granted per unit of effort for a single graph. Effort is calibrated so that
/// a validation set of ~1,500 graphs takes roughly `effort` milliseconds in total.
const SEARCH_NANOS_PER_EFFORT: u64 = 400;
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
}

impl Policy {
    fn graph_order(graph: &OpGraph) -> Self {
        let n = graph.total_ops() as usize;
        Self {
            priority: (0..n as u32).map(|i| i * PRIORITY_SPACING).collect(),
            flips: vec![FlipChoice::Cheapest; n],
            pre_shuffle: false,
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
    let trees = build_tree_graph(graph);
    let policy = search_policy(finalization, shuffle, &trees.graph, effort);

    let mut ops = Vec::new();
    let new_next_alloc_id = greedy_schedule(
        |op| ops.push(op),
        finalization,
        next_alloc_id,
        shuffle,
        &trees.graph,
        &policy,
        None,
        effort > 0,
    )
    .expect("unbounded schedule cannot be cut off");
    let spill_count = new_next_alloc_id - next_alloc_id;

    let ops = trees.expand_schedule(graph, &ops);

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

fn policy_gas(
    finalization: BlockFinalization,
    config: ShuffleConfig,
    graph: &OpGraph,
    policy: &Policy,
    bound: Option<u64>,
) -> Option<u64> {
    let gas = Cell::new(0u64);
    greedy_schedule(
        |op| gas.set(gas.get() + u64::from(op.gas_cost(config))),
        finalization,
        StaticAllocId::ZERO,
        config,
        graph,
        policy,
        bound.map(|bound| (&gas, bound)),
        false,
    )?;
    Some(gas.get())
}

/// Hill climbing over [`Policy`] genomes: mutate the best genome, rerun the greedy scheduler and
/// keep the mutant if it is no more expensive. Equal-cost mutants are accepted to drift across
/// plateaus.
fn search_policy(
    finalization: BlockFinalization,
    config: ShuffleConfig,
    graph: &OpGraph,
    effort: u32,
) -> Policy {
    let mut best = Policy::graph_order(graph);
    let n = graph.total_ops() as usize;
    if effort == 0 || n == 0 {
        return best;
    }
    let flippable: Vec<usize> = graph
        .op_ids()
        .filter(|&op| matches!(graph.get_op(op).kind, OpNodeKind::Flippable(_)))
        .map(|op| op.idx())
        .collect();


    let start = Instant::now();
    let budget = Duration::from_nanos(u64::from(effort) * SEARCH_NANOS_PER_EFFORT);
    let mut best_gas = policy_gas(finalization, config, graph, &best, None).expect("unbounded");
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (n as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
    let stagnation_limit = STAGNATION_MIN.max(STAGNATION_PER_OP * n as u32);
    let restarts_allowed = n + flippable.len() >= MIN_CHOICES_FOR_RESTARTS;
    let mut stagnant = 0u32;
    let mut current = best.clone();
    let mut current_gas = best_gas;
    let mut candidate = best.clone();

    while best_gas > 0 && start.elapsed() < budget {
        if stagnant >= stagnation_limit {
            if !restarts_allowed {
                break;
            }
            current.clone_from(&best);
            for _ in 0..RESTART_KICK_BASE + n / RESTART_KICK_OPS_PER_MUTATION {
                mutate(&mut current, &flippable, &mut rng);
            }
            current_gas = policy_gas(finalization, config, graph, &current, None).expect("unbounded");
            stagnant = 0;
        }
        candidate.priority.clone_from(&current.priority);
        candidate.flips.clone_from(&current.flips);
        candidate.pre_shuffle = current.pre_shuffle;
        let mutations = 1 + rng.below(3).saturating_sub(1);
        for _ in 0..mutations {
            mutate(&mut candidate, &flippable, &mut rng);
        }
        stagnant += 1;
        if let Some(gas) = policy_gas(finalization, config, graph, &candidate, Some(current_gas)) {
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

fn mutate(policy: &mut Policy, flippable: &[usize], rng: &mut Rng) {
    let n = policy.priority.len();
    if rng.below(16) == 0 {
        policy.pre_shuffle = !policy.pre_shuffle;
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
    policy: &Policy,
    bound: Option<(&Cell<u64>, u64)>,
    thorough_final_shuffle: bool,
) -> Option<StaticAllocId> {
    let exceeded = || bound.is_some_and(|(gas, bound)| gas.get() > bound);
    let mut completable_backing = SmallVec::<[BitsetWord; SCRATCH_OP_SET_INLINE_CAPACITY]>::new();
    completable_backing.resize(graph.words_per_set() as usize, 0);
    let mut completable = OpSetMut::new(&mut completable_backing, graph.total_ops());

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
    if policy.pre_shuffle {
        greedy_shuffler::shuffle_to(config, &mut stack, &pre_shuffle_layout(graph, policy), false);
    }

    'schedule: loop {
        completable.clear();
        graph.collect_next_completable_into(complete.as_ref(), &mut completable);
        // Cheapest-next-op selection is myopic: on the corpus it is 4-6% worse than
        // taking ready ops in graph order, which tracks the tree graph's operand order.
        let Some(op) = completable.iter().min_by_key(|op| (policy.priority[op.idx()], op.idx()))
        else {
            break 'schedule;
        };
        let flipped = matches!(graph.get_op(op).kind, OpNodeKind::Flippable(_))
            && match policy.flips[op.idx()] {
                FlipChoice::Straight => false,
                FlipChoice::Flipped => true,
                FlipChoice::Cheapest => {
                    let op_cost = |flipped| {
                        let mut ops = Vec::new();
                        let mut trial_stack = stack.clone_with(|op| ops.push(op));
                        greedy_schedule_op(
                            config,
                            &mut trial_stack,
                            graph,
                            op,
                            complete.as_ref(),
                            flipped,
                        );
                        crate::stack::gas_cost(&ops, config)
                    };
                    op_cost(true) < op_cost(false)
                }
            };
        greedy_schedule_op(config, &mut stack, graph, op, complete.as_ref(), flipped);
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

/// Input layout that lets operations consume their inputs straight off the top: inputs feeding
/// operations come first in the order the policy consumes them, followed by the inputs passed
/// through to the block outputs, already in output order. Doing this permutation once up front is
/// often cheaper than fixing up pass-through values at the end with deep swaps.
fn pre_shuffle_layout(graph: &OpGraph, policy: &Policy) -> Vec<ValueNodeId> {
    let mut passed_through = Vec::new();
    for &value in graph.output_values_fifo().iter().rev() {
        if graph.is_input(value) && !passed_through.contains(&value) {
            passed_through.push(value);
        }
    }
    passed_through.reverse();

    let mut order: Vec<OpNodeId> = graph.op_ids().collect();
    order.sort_by_key(|op| (policy.priority[op.idx()], op.idx()));
    let mut layout = Vec::new();
    for op in order {
        for &value in graph.get_op(op).inputs_fifo {
            if graph.is_input(value) && !layout.contains(&value) && !passed_through.contains(&value)
            {
                layout.push(value);
            }
        }
    }
    layout.extend(passed_through);
    layout
}
