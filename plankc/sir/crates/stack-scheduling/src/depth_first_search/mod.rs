use std::{num::NonZero, rc::Rc};

use hashbrown::HashMap;
use plank_core::IndexVec;
use sir_data::StaticAllocId;
use smallvec::SmallVec;

use crate::{
    BlockFinalization,
    greedy_intra_op_scheduler::greedy_schedule_op,
    greedy_shuffler,
    op_graph::{BitsetWord, OpGraph, OpNodeId, OpSet, OpSetMut, ValueNodeId},
    scheduler::greedy_schedule,
    stack::{ShuffleConfig, StackOps, TrackedStack},
};

const BASE_COST_FACTOR: u32 = 100;
const SCRATCH_OP_SET_INLINE_CAPACITY: usize = 512 / BitsetWord::BITS as usize;
const ESTIMATED_STACK_OPS_PER_GRAPH_OP: usize = 8;

#[derive(Clone, Copy)]
pub struct SearchConfig {
    pub max_candidates: NonZero<usize>,
}

pub struct SearchResult {
    pub ops: Box<[StackOps]>,
    pub spill_count: u32,
    pub candidate_limit_reached: bool,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct SearchState {
    complete: Box<[BitsetWord]>,
    values: Box<[ValueNodeId]>,
    stack_end: usize,
}

struct SearchNode {
    state: Rc<SearchState>,
    completed_count: u32,
    executed_cost: u32,
}

struct Child {
    node: SearchNode,
    transition_ops: Box<[StackOps]>,
    lower_bound: u32,
}

struct Search<'a> {
    finalization: BlockFinalization,
    graph: &'a OpGraph,
    next_alloc_id: StaticAllocId,
    shuffle: ShuffleConfig,
    max_candidates: usize,
    assessed_candidates: usize,
    candidate_limit_reached: bool,
    best_cost: u32,
    best_ops: Box<[StackOps]>,
    best_spill_count: u32,
    path: Vec<StackOps>,
    best_state_costs: HashMap<Rc<SearchState>, u32>,
}

pub fn schedule(
    finalization: BlockFinalization,
    next_alloc_id: StaticAllocId,
    shuffle: ShuffleConfig,
    config: SearchConfig,
    graph: &OpGraph,
) -> SearchResult {
    let mut incumbent_ops = Vec::new();
    let incumbent_next_alloc_id =
        greedy_schedule(|op| incumbent_ops.push(op), finalization, next_alloc_id, shuffle, graph);
    let incumbent_cost = stack_ops_cost(&incumbent_ops, shuffle);
    let incumbent_spill_count = incumbent_next_alloc_id - next_alloc_id;

    if graph.total_ops() == 0 {
        return SearchResult {
            ops: incumbent_ops.into_boxed_slice(),
            spill_count: incumbent_spill_count,
            candidate_limit_reached: false,
        };
    }

    let start = SearchNode {
        state: Rc::new(SearchState {
            complete: vec![0; graph.words_per_set() as usize].into_boxed_slice(),
            values: graph.input_values_fifo().iter().collect(),
            stack_end: graph.input_values_fifo().len() as usize,
        }),
        completed_count: 0,
        executed_cost: 0,
    };
    let mut search = Search {
        finalization,
        graph,
        next_alloc_id,
        shuffle,
        max_candidates: config.max_candidates.get(),
        assessed_candidates: 0,
        candidate_limit_reached: false,
        best_cost: incumbent_cost,
        best_ops: incumbent_ops.into_boxed_slice(),
        best_spill_count: incumbent_spill_count,
        path: Vec::with_capacity(graph.total_ops() as usize * ESTIMATED_STACK_OPS_PER_GRAPH_OP),
        best_state_costs: HashMap::new(),
    };
    search.visit(start);

    SearchResult {
        ops: search.best_ops,
        spill_count: search.best_spill_count,
        candidate_limit_reached: search.candidate_limit_reached,
    }
}

impl Search<'_> {
    fn visit(&mut self, node: SearchNode) {
        if !record_if_improved(&mut self.best_state_costs, &node.state, node.executed_cost) {
            return;
        }

        if node.completed_count == self.graph.total_ops() {
            self.finish(node);
            return;
        }
        if self.assessed_candidates == self.max_candidates {
            self.candidate_limit_reached = true;
            return;
        }

        let complete = OpSet::new(&node.state.complete, self.graph.total_ops());
        let mut completable_backing =
            SmallVec::<[BitsetWord; SCRATCH_OP_SET_INLINE_CAPACITY]>::new();
        completable_backing.resize(self.graph.words_per_set() as usize, 0);
        let mut completable = OpSetMut::new(&mut completable_backing, self.graph.total_ops());
        self.graph.collect_next_completable_into(complete, &mut completable);
        let completable = completable.iter().collect::<SmallVec<[OpNodeId; 32]>>();

        let mut children = Vec::with_capacity(completable.len());
        for op in completable {
            if self.assessed_candidates == self.max_candidates {
                self.candidate_limit_reached = true;
                break;
            }
            self.assessed_candidates += 1;
            let child = self.build_child(&node, complete, op);
            if child.lower_bound >= self.best_cost {
                continue;
            }
            children.push(child);
        }
        children.sort_unstable_by_key(|child| child.lower_bound);

        for child in children {
            let path_len = self.path.len();
            self.path.extend_from_slice(&child.transition_ops);
            self.visit(child.node);
            self.path.truncate(path_len);
        }
    }

    fn build_child(&self, node: &SearchNode, complete: OpSet<'_>, op: OpNodeId) -> Child {
        let mut transition_ops = Vec::with_capacity(ESTIMATED_STACK_OPS_PER_GRAPH_OP);
        let mut stack = TrackedStack::new_from_parts(
            self.next_alloc_id,
            |op| transition_ops.push(op),
            &node.state.values[..node.state.stack_end],
            node.state.values[node.state.stack_end..].to_vec(),
        );
        greedy_schedule_op(self.shuffle, &mut stack, self.graph, op, complete, false);

        let complete = {
            let mut backing = complete.clone_backing();
            OpSetMut::new(&mut backing, self.graph.total_ops()).add(op);
            backing.into_boxed_slice()
        };
        let completed = OpSet::new(&complete, self.graph.total_ops());
        if self.finalization == BlockFinalization::ShuffleToOutputs {
            while stack.top().is_some_and(|value| self.graph.uses_remaining(completed, value) == 0)
            {
                stack.pop();
            }
        }

        let values = [stack.fifo(), stack.underlying_spilled()].concat();
        let stack_end = stack.fifo().len();
        drop(stack);
        let transition_cost = stack_ops_cost(&transition_ops, self.shuffle);
        let executed_cost = node.executed_cost + transition_cost;
        let remaining_cost = remaining_cost_lower_bound(
            &complete,
            &values[..stack_end],
            &values[stack_end..],
            self.finalization == BlockFinalization::ShuffleToOutputs,
            self.graph,
        );

        Child {
            node: SearchNode {
                state: Rc::new(SearchState {
                    complete,
                    values: values.into_boxed_slice(),
                    stack_end,
                }),
                completed_count: node.completed_count + 1,
                executed_cost,
            },
            transition_ops: transition_ops.into_boxed_slice(),
            lower_bound: executed_cost + remaining_cost,
        }
    }

    fn finish(&mut self, node: SearchNode) {
        let mut final_ops = Vec::new();
        let mut stack = TrackedStack::new_from_parts(
            self.next_alloc_id,
            |op| final_ops.push(op),
            &node.state.values[..node.state.stack_end],
            node.state.values[node.state.stack_end..].to_vec(),
        );
        if self.finalization == BlockFinalization::ShuffleToOutputs {
            greedy_shuffler::shuffle(self.shuffle, &mut stack, self.graph);
        }
        let spill_count = u32::try_from(stack.underlying_spilled().len()).expect("overflow");
        drop(stack);

        let cost = node.executed_cost + stack_ops_cost(&final_ops, self.shuffle);
        if cost >= self.best_cost {
            return;
        }

        self.best_cost = cost;
        let mut best_ops = Vec::with_capacity(self.path.len() + final_ops.len());
        best_ops.extend_from_slice(&self.path);
        best_ops.extend_from_slice(&final_ops);
        self.best_ops = best_ops.into_boxed_slice();
        self.best_spill_count = spill_count;
    }
}

fn record_if_improved(
    best_state_costs: &mut HashMap<Rc<SearchState>, u32>,
    state: &Rc<SearchState>,
    cost: u32,
) -> bool {
    if best_state_costs.get(state).is_some_and(|&best_cost| best_cost <= cost) {
        return false;
    }
    best_state_costs.insert(state.clone(), cost);
    true
}

fn remaining_cost_lower_bound(
    complete: &[BitsetWord],
    stack: &[ValueNodeId],
    spilled: &[ValueNodeId],
    needs_final_shuffle: bool,
    graph: &OpGraph,
) -> u32 {
    const COPY_COST: u32 = 3 * BASE_COST_FACTOR;
    const LOAD_COST: u32 = 6 * BASE_COST_FACTOR;

    let complete = OpSet::new(complete, graph.total_ops());
    let mut demand = IndexVec::<ValueNodeId, u32>::from_vec(vec![0; graph.total_values() as usize]);
    for op in graph.op_ids().filter(|&op| !complete.contains(op)) {
        for &input in graph.get_op(op).inputs_fifo {
            demand[input] += 1;
        }
    }
    if needs_final_shuffle {
        for &output in graph.output_values_fifo() {
            demand[output] += 1;
        }
    }

    // A producer or block input supplies one free copy. Every further demand requires at least a
    // DUP-cost operation, while a value present only in spill memory first requires a LOAD.
    let demand_cost = demand
        .enumerate_idx()
        .map(|(value, &demand)| {
            let source_cost =
                u32::from(demand > 0 && !stack.contains(&value) && spilled.contains(&value))
                    * LOAD_COST;
            source_cost + demand.saturating_sub(1) * COPY_COST
        })
        .sum::<u32>();
    // Values with no demand cannot be consumed by future operations, so finalizing a
    // non-terminating block must remove each with at least a POP-cost operation.
    let cleanup_cost = if needs_final_shuffle {
        u32::try_from(stack.iter().filter(|&&value| demand[value] == 0).count()).expect("overflow")
            * COPY_COST
    } else {
        0
    };

    demand_cost + cleanup_cost
}

fn stack_ops_cost(ops: &[StackOps], shuffle: ShuffleConfig) -> u32 {
    ops.iter()
        .map(|&op| {
            let cost = match op {
                // These represent necessary basic block operations and therefore shouldn't be
                // included in the scheduling cost.
                StackOps::Flipped(_) | StackOps::Op(_) | StackOps::CallRetPush(_) => 0,
                StackOps::Swap(_) | StackOps::Dup(_) | StackOps::Pop => 3,
                StackOps::Exchange(_, _) => shuffle.exchange_cost,
                // Conservatively assume store will need to pay for memory expansion.
                StackOps::Store(_) => 9,
                StackOps::Load(_) => 6,
            };
            u32::from(cost) * BASE_COST_FACTOR
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_revisits_a_state_at_lower_cost() {
        let state =
            Rc::new(SearchState { complete: Box::new([]), values: Box::new([]), stack_end: 0 });
        let equal_state =
            Rc::new(SearchState { complete: Box::new([]), values: Box::new([]), stack_end: 0 });
        let mut best_state_costs = HashMap::new();

        assert!(record_if_improved(&mut best_state_costs, &state, 10));
        assert!(!record_if_improved(&mut best_state_costs, &equal_state, 10));
        assert!(!record_if_improved(&mut best_state_costs, &equal_state, 11));
        assert!(record_if_improved(&mut best_state_costs, &equal_state, 9));
    }
}
