//! Inspired by the "Treegraph-based Instruction Scheduling for Stack-based Virtual Machines" by J.
//! Park, J. Park, W. Song et. al.
//!
//! Adapted to take effects into account, as well as taking advantage of flippable operations.

use crate::{
    op_graph::{
        OpGraph, OpGraphBuilder, OpNodeId, OpNodeKind, OpView, ValueNodeId, builder::AddingGraphOps,
    },
    stack::StackOps,
};
use itertools::Itertools;
use plank_core::{DenseIndexMap, DenseIndexSet, IndexVec};
use sir_data::OperationIdx;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeStep {
    pub operation: OpNodeId,
    pub flipped: bool,
}

#[derive(Debug)]
pub struct TreeGraph {
    pub graph: OpGraph,
    flipped: DenseIndexSet<OpNodeId>,
    trees: IndexVec<OperationIdx, Tree>,
}

impl TreeGraph {
    pub fn original_operations(
        &self,
        original: &OpGraph,
        operation: OpNodeId,
    ) -> impl Iterator<Item = TreeStep> {
        self.trees[operation]
            .iter()
            .map(|&operation| TreeStep { operation, flipped: self.flipped.contains(operation) })
    }

    pub(crate) fn expand_schedule(
        &self,
        original: &OpGraph,
        schedule: &[StackOps],
    ) -> Box<[StackOps]> {
        todo!()
    }
}

#[derive(Debug)]
struct Tree {
    root: OpNodeId,
    folded_count: u32,
}

struct AccumulatorState<'g> {
    total_ops: usize,
    total_values: usize,
    original: &'g OpGraph,
    builder: OpGraphBuilder<AddingGraphOps>,
    values_og_to_new: DenseIndexMap<ValueNodeId, ValueNodeId>,
    op_og_to_new: DenseIndexMap<OpNodeId, OpNodeId>,

    trees: IndexVec<OperationIdx, Tree>,
}

struct PendingTree<'g> {
    members: DenseIndexSet<OpNodeId>,
    root: OpNodeId,
    view: OpView<'g>,
    folded_count: usize,
}

impl<'g> AccumulatorState<'g> {
    fn tree_is_valid_successor(
        &self,
        tree_members: &DenseIndexSet<OpNodeId>,
        deeper_inputs: &[ValueNodeId],
    ) -> bool {
        for member in tree_members.iter() {
            for &next in deeper_inputs {
                let Some(producer) = self.original.get_producer(next) else { continue };
                if self.original.get_op(producer).predecessors.contains(member) {
                    return false;
                }
            }
        }
        true
    }

    fn materialize_pending(&mut self, pending: PendingTree<'_>) {
        let folded_count = pending.folded_count.try_into().expect("overflow");
        let virtual_op_idx = self.trees.push(Tree { root: pending.root, folded_count });

        let mut new_op = self.builder.begin_op(match pending.view.kind {
            OpNodeKind::Flippable(_) if folded_count == 0 => OpNodeKind::Flippable(virtual_op_idx),
            _ => OpNodeKind::Normal(virtual_op_idx),
        });
        self.op_og_to_new.insert(pending.root, new_op.id());

        for &input in &pending.view.inputs_fifo[pending.folded_count..] {
            new_op.add_input(self.values_og_to_new[input]);
        }
        for pred in pending.view.predecessors.iter() {
            new_op.add_predecessor(pred);
        }
        let mut new_op = new_op.end_inputs_begin_outputs();
        for &output in pending.view.outputs_fifo {
            self.values_og_to_new.insert(output, new_op.add_output());
        }
    }

    fn try_fold(&mut self, root: OpNodeId) -> Option<PendingTree<'g>> {
        if self.op_og_to_new.contains(root) {
            return None;
        }

        let view = self.original.get_op(root);
        let mut members = DenseIndexSet::with_capacity_in_bits(self.total_ops);
        let mut folded_count = 0;

        for &input in view.inputs_fifo {
            if self.values_og_to_new.contains(input) {
                break;
            }
            let producer = self.original.get_producer(input).expect("unmapped input");

            let Some(pending) = self.try_fold(producer) else { break };

            let deeper_inputs = &view.inputs_fifo[folded_count + 1..];
            if !self.tree_is_valid_successor(&pending.members, deeper_inputs) {
                self.materialize_pending(pending);
                break;
            }

            members.union_with(&pending.members);
            folded_count += 1;
        }

        let pending = PendingTree { members, root, view, folded_count };

        if folded_count == view.inputs_fifo.len()
            && let &[output] = view.outputs_fifo
            && !self.original.output_values_fifo().contains(&output)
            && let Ok(sole_consumer) = self.original.get_consumers(output).iter().exactly_one()
            && let consumer = self.original.get_op(sole_consumer)
            && consumer.inputs_fifo.iter().filter(|&&v| v == output).count() == 1
        {
            return Some(pending);
        }

        self.materialize_pending(pending);

        None
    }
}

pub fn build_tree_graph(original: &OpGraph) -> TreeGraph {
    let total_ops = original.total_ops() as usize;
    let total_values = original.total_values() as usize;
    let mut flipped = DenseIndexSet::with_capacity_in_bits(total_ops);
    let mut values_og_to_new = DenseIndexMap::with_capacity(total_values);

    let mut builder = OpGraphBuilder::with_capacity(total_ops, total_values);
    for input in original.input_values_fifo() {
        let new_input = builder.push_input_value();
        values_og_to_new.insert(input, new_input);
    }
    let builder = builder.end_inputs_begin_ops();

    let mut state = AccumulatorState {
        total_ops,
        total_values,
        original,
        builder,
        values_og_to_new,
        op_og_to_new: DenseIndexMap::with_capacity(total_ops),
        trees: IndexVec::with_capacity(total_ops),
    };

    for id in original.op_ids() {
        state.try_fold(id);
    }

    TreeGraph { graph: todo!(), flipped: state.flipped, trees: todo!() }
}

#[cfg(test)]
mod tests;
