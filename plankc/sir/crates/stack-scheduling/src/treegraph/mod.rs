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

#[derive(Debug, Clone, Copy)]
struct Tree {
    root: OpNodeId,
    folded_count: u32,
}

impl TreeGraph {
    pub fn original_operations(&self, original: &OpGraph, operation: OpNodeId) -> Vec<TreeStep> {
        let idx = match self.graph.op_kind(operation) {
            OpNodeKind::Flippable(idx) | OpNodeKind::Normal(idx) => idx,
            OpNodeKind::RetDestPush(_) => unreachable!("treegraph doesn't add `RetDestPush`"),
        };
        let tree = self.trees[idx];

        let mut steps = Vec::new();

        fn iter_ops(
            steps: &mut Vec<TreeStep>,
            tg: &TreeGraph,
            original: &OpGraph,
            root: OpNodeId,
            folded_count: u32,
        ) {
            let op = original.get_op(root);
            for &input in op.inputs_fifo[..folded_count as usize].iter().rev() {
                let producer = original.get_producer(input).expect("tree references bb input");
                iter_ops(steps, tg, original, producer, original.op_input_count(producer))
            }
            steps.push(TreeStep { operation: root, flipped: tg.flipped.contains(root) })
        }

        iter_ops(&mut steps, self, original, tree.root, tree.folded_count);

        steps
    }

    fn expand_tree(
        &self,
        original: &OpGraph,
        ops: &mut Vec<StackOps>,
        root: OpNodeId,
        folded_count: u32,
        flipped: bool,
    ) {
        let op = original.get_op(root);
        for &input in op.inputs_fifo[..folded_count as usize].iter().rev() {
            let producer = original.get_producer(input).expect("tree member consumes input");
            self.expand_tree(
                original,
                ops,
                producer,
                original.op_input_count(producer),
                self.flipped.contains(producer),
            );
        }
        let op = match op.kind {
            OpNodeKind::Flippable(idx) if flipped => StackOps::Flipped(idx),
            OpNodeKind::Normal(idx) | OpNodeKind::Flippable(idx) => StackOps::Op(idx),
            OpNodeKind::RetDestPush(idx) => StackOps::CallRetPush(idx),
        };
        ops.push(op);
    }

    pub(crate) fn expand_schedule(
        &self,
        original: &OpGraph,
        schedule: &[StackOps],
    ) -> Box<[StackOps]> {
        let mut expanded = Vec::new();

        for &op in schedule {
            let (flipped, idx) = match op {
                StackOps::Flipped(idx) => (true, idx),
                StackOps::Op(idx) => (false, idx),
                StackOps::CallRetPush(_) => {
                    unreachable!("treegraph builder maps all ops to normal ops")
                }
                op => {
                    expanded.push(op);
                    continue;
                }
            };

            let tree = self.trees[idx];
            self.expand_tree(original, &mut expanded, tree.root, tree.folded_count, flipped);
        }

        expanded.into_boxed_slice()
    }
}

struct AccumulatorState<'g> {
    total_ops: usize,
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
            new_op.add_predecessor(self.op_og_to_new[pred]);
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
    let mut values_og_to_new = DenseIndexMap::with_capacity(total_values);

    let mut builder = OpGraphBuilder::with_capacity(total_ops, total_values);
    for input in original.input_values_fifo() {
        let new_input = builder.push_input_value();
        values_og_to_new.insert(input, new_input);
    }
    let builder = builder.end_inputs_begin_ops();

    let mut state = AccumulatorState {
        total_ops,
        original,
        builder,
        values_og_to_new,
        op_og_to_new: DenseIndexMap::with_capacity(total_ops),
        trees: IndexVec::with_capacity(total_ops),
    };

    for id in original.op_ids() {
        if let Some(pending) = state.try_fold(id) {
            state.materialize_pending(pending);
        }
    }

    let mut tg = state.builder.end_ops_begin_end_stack();
    for &output in original.output_values_fifo() {
        tg.push_end_stack_value(state.values_og_to_new[output]);
    }

    TreeGraph { graph: tg.finish(), flipped: DenseIndexSet::new(), trees: state.trees }
}

#[cfg(test)]
mod tests;
