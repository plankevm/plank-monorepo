//! Inspired by the "Treegraph-based Instruction Scheduling for Stack-based Virtual Machines" by J.
//! Park, J. Park, W. Song et. al.
//!
//! Adapted to take effects into account, as well as taking advantage of flippable operations.

use crate::{
    op_graph::{
        OpGraph, OpGraphBuilder, OpNodeId, OpNodeKind, ValueNodeId, builder::AddingGraphOps,
    },
    stack::StackOps,
};
use plank_core::{DenseIndexMap, DenseIndexSet, IndexVec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeStep {
    pub operation: OpNodeId,
    pub flipped: bool,
}

#[derive(Debug)]
pub struct TreeGraph {
    pub graph: OpGraph,
    flipped: DenseIndexSet<OpNodeId>,
    trees: IndexVec<OpNodeId, Vec<OpNodeId>>,
}

impl TreeGraph {
    pub fn original_operations(&self, operation: OpNodeId) -> impl Iterator<Item = TreeStep> {
        self.trees[operation]
            .iter()
            .map(|&operation| TreeStep { operation, flipped: self.flipped.contains(operation) })
    }

    pub(crate) fn expand_schedule(
        &self,
        original: &OpGraph,
        schedule: &[StackOps],
    ) -> Box<[StackOps]> {
        let mut operations = DenseIndexMap::with_capacity(self.graph.total_ops() as usize);
        let mut return_dest_pushes = DenseIndexMap::with_capacity(self.graph.total_ops() as usize);
        for tree_operation in self.graph.op_ids() {
            match self.graph.get_op(tree_operation).kind {
                OpNodeKind::Flippable(operation) | OpNodeKind::Normal(operation) => {
                    operations.insert_no_prev(operation, tree_operation);
                }
                OpNodeKind::RetDestPush(operation) => {
                    return_dest_pushes.insert_no_prev(operation, tree_operation);
                }
            }
        }

        let mut expanded = Vec::with_capacity(original.total_ops() as usize + schedule.len());
        for &scheduled in schedule {
            let (tree_operation, externally_flipped) = match scheduled {
                StackOps::Op(operation) => (operations.get(operation).copied(), false),
                StackOps::Flipped(operation) => (operations.get(operation).copied(), true),
                StackOps::CallRetPush(operation) => {
                    (return_dest_pushes.get(operation).copied(), false)
                }
                operation => {
                    expanded.push(operation);
                    continue;
                }
            };
            let tree_operation =
                tree_operation.expect("scheduled operation missing from tree graph");
            if externally_flipped {
                assert!(matches!(self.graph.get_op(tree_operation).kind, OpNodeKind::Flippable(_)));
            }
            let root = *self.trees[tree_operation].last().expect("empty tree");
            expanded.extend(self.original_operations(tree_operation).map(|step| {
                let flipped = step.flipped ^ (externally_flipped && step.operation == root);
                match original.get_op(step.operation).kind {
                    OpNodeKind::Flippable(operation) if flipped => StackOps::Flipped(operation),
                    OpNodeKind::Flippable(operation) | OpNodeKind::Normal(operation) => {
                        StackOps::Op(operation)
                    }
                    OpNodeKind::RetDestPush(operation) => StackOps::CallRetPush(operation),
                }
            }));
        }
        expanded.into_boxed_slice()
    }
}

pub fn build_tree_graph(original: &OpGraph) -> TreeGraph {
    let total_ops = original.total_ops() as usize;
    let total_values = original.total_values() as usize;
    let mut producers = DenseIndexMap::with_capacity(total_values);
    let mut use_counts = DenseIndexMap::with_capacity(total_values);

    for operation in original.op_ids() {
        let op = original.get_op(operation);
        for &output in op.outputs_fifo {
            producers.insert_no_prev(output, operation);
        }
        for &input in op.inputs_fifo {
            *use_counts.get_or_insert_with(input, || 0u32) += 1;
        }
    }

    let mut fold_parents = DenseIndexMap::with_capacity(total_ops);
    for consumer in original.op_ids() {
        for &input in original.get_op(consumer).inputs_fifo {
            let Some(&producer) = producers.get(input) else {
                continue;
            };
            if original.get_op(producer).outputs_fifo.len() == 1
                && use_counts.get(input) == Some(&1)
                && !original.output_values_fifo().contains(&input)
            {
                fold_parents.insert_no_prev(producer, consumer);
            }
        }
    }

    let mut original_to_value = DenseIndexMap::with_capacity(total_values);
    let mut builder = OpGraphBuilder::with_capacity(total_ops, total_values);
    for input in original.input_values_fifo() {
        original_to_value.insert_no_prev(input, builder.push_input_value());
    }

    TreeGraphBuilder {
        original,
        producers,
        fold_parents,
        completed: DenseIndexMap::with_capacity(total_ops),
        materialization_order: Vec::with_capacity(total_ops),
        original_to_tree: DenseIndexMap::with_capacity(total_ops),
        ancestors: collect_ancestors(original),
        builder: builder.end_inputs_begin_ops(),
        original_to_value,
        trees: IndexVec::with_capacity(total_ops),
        flipped: DenseIndexSet::with_capacity_in_bits(total_ops),
        emitted: DenseIndexMap::with_capacity(total_ops),
        emitting: DenseIndexSet::with_capacity_in_bits(total_ops),
    }
    .fold_ops()
    .into_graph()
}

struct TreeGraphBuilder<'graph> {
    original: &'graph OpGraph,
    producers: DenseIndexMap<ValueNodeId, OpNodeId>,
    fold_parents: DenseIndexMap<OpNodeId, OpNodeId>,
    completed: DenseIndexMap<OpNodeId, Vec<OpNodeId>>,
    materialization_order: Vec<OpNodeId>,
    original_to_tree: DenseIndexMap<OpNodeId, OpNodeId>,
    ancestors: IndexVec<OpNodeId, DenseIndexSet<OpNodeId>>,
    builder: OpGraphBuilder<AddingGraphOps>,
    original_to_value: DenseIndexMap<ValueNodeId, ValueNodeId>,
    trees: IndexVec<OpNodeId, Vec<OpNodeId>>,
    flipped: DenseIndexSet<OpNodeId>,
    emitted: DenseIndexMap<OpNodeId, OpNodeId>,
    emitting: DenseIndexSet<OpNodeId>,
}

impl TreeGraphBuilder<'_> {
    fn fold_op(&mut self, root: OpNodeId) -> Vec<OpNodeId> {
        let op = self.original.get_op(root);
        let operands = op
            .inputs_fifo
            .iter()
            .map(|&input| {
                self.producers.get(input).copied().and_then(|producer| {
                    (self.fold_parents.get(producer) == Some(&root)).then(|| self.fold_op(producer))
                })
            })
            .collect::<Vec<_>>();

        let normal = self.plan_tree(root, &operands, false);
        let (steps, root_flipped) =
            if matches!(op.kind, OpNodeKind::Flippable(_)) && op.inputs_fifo.len() >= 2 {
                let flipped = self.plan_tree(root, &operands, true);
                if flipped.len() > normal.len() { (flipped, true) } else { (normal, false) }
            } else {
                (normal, false)
            };
        if root_flipped {
            assert!(self.flipped.add(root));
        }

        for operand in operands.into_iter().flatten() {
            if !steps.contains(operand.last().unwrap()) {
                self.materialize(operand);
            }
        }
        steps
    }

    fn plan_tree(
        &self,
        root: OpNodeId,
        operands: &[Option<Vec<OpNodeId>>],
        root_flipped: bool,
    ) -> Vec<OpNodeId> {
        let mut steps = Vec::new();
        let mut operand_lengths = Vec::new();
        let order = (0..operands.len())
            .rev()
            .map(|position| if root_flipped && position < 2 { 1 - position } else { position });

        for position in order {
            let Some(operand) = &operands[position] else {
                steps.clear();
                operand_lengths.clear();
                continue;
            };
            steps.extend_from_slice(operand);
            operand_lengths.push(operand.len());
            if !self.is_valid_tree(&steps, false) {
                steps.clear();
                steps.extend_from_slice(operand);
                operand_lengths.clear();
                operand_lengths.push(operand.len());
            }
        }

        loop {
            steps.push(root);
            if self.is_valid_tree(&steps, root_flipped) {
                return steps;
            }
            steps.pop();
            steps.drain(..operand_lengths.remove(0));
        }
    }

    fn is_valid_tree(&self, steps: &[OpNodeId], root_flipped: bool) -> bool {
        let mut operations =
            DenseIndexSet::with_capacity_in_bits(self.original.total_ops() as usize);
        for &operation in steps {
            assert!(operations.add(operation));
        }

        let mut seen = DenseIndexSet::with_capacity_in_bits(self.original.total_ops() as usize);
        for &operation in steps {
            for predecessor in self.original.get_predecessors(operation).iter() {
                if operations.contains(predecessor) && !seen.contains(predecessor) {
                    return false;
                }
                if !operations.contains(predecessor)
                    && self.ancestors[predecessor]
                        .iter()
                        .any(|ancestor| operations.contains(ancestor))
                {
                    return false;
                }
            }
            seen.add(operation);
        }

        let mut stack = Vec::new();
        for (step_position, &step) in steps.iter().enumerate() {
            let op = self.original.get_op(step);
            let flipped =
                self.flipped.contains(step) || (root_flipped && step_position == steps.len() - 1);
            for position in 0..op.inputs_fifo.len() {
                let position = if flipped && position < 2 { 1 - position } else { position };
                match stack.last() {
                    Some(&actual) if actual == op.inputs_fifo[position] => {
                        stack.pop();
                    }
                    Some(_) => return false,
                    None => {}
                }
            }
            stack.extend(op.outputs_fifo.iter().rev().copied());
        }
        true
    }

    fn required_inputs(&self, steps: &[OpNodeId]) -> Vec<ValueNodeId> {
        let mut stack = Vec::new();
        let mut inputs_fifo = Vec::new();
        for &step in steps {
            let op = self.original.get_op(step);
            for position in 0..op.inputs_fifo.len() {
                let position = if self.flipped.contains(step) && position < 2 {
                    1 - position
                } else {
                    position
                };
                let input = op.inputs_fifo[position];
                if stack.last() == Some(&input) {
                    stack.pop();
                } else {
                    assert!(stack.is_empty(), "materialized an invalid tree");
                    inputs_fifo.push(input);
                }
            }
            stack.extend(op.outputs_fifo.iter().rev().copied());
        }
        inputs_fifo
    }

    fn materialize(&mut self, tree: Vec<OpNodeId>) -> OpNodeId {
        let root = *tree.last().expect("empty tree");
        for &operation in &tree {
            self.original_to_tree.insert_no_prev(operation, root);
        }
        self.completed.insert_no_prev(root, tree);
        self.materialization_order.push(root);
        root
    }

    fn fold_ops(mut self) -> Self {
        for operation in self.original.op_ids() {
            if self.fold_parents.contains(operation) {
                continue;
            }
            let tree = self.fold_op(operation);
            self.materialize(tree);
        }
        assert_eq!(self.original_to_tree.iter().count(), self.original.total_ops() as usize);
        self
    }

    fn emit_tree(&mut self, root: OpNodeId) -> OpNodeId {
        if let Some(&operation) = self.emitted.get(root) {
            return operation;
        }
        assert!(self.emitting.add(root), "cycle in tree graph");

        let mut predecessors =
            DenseIndexSet::with_capacity_in_bits(self.original.total_ops() as usize);
        for &operation in &self.completed[root] {
            for predecessor in self.original.get_predecessors(operation).iter() {
                let predecessor_tree = self.original_to_tree[predecessor];
                if predecessor_tree != root {
                    predecessors.add(predecessor_tree);
                }
            }
        }
        for predecessor in &predecessors {
            self.emit_tree(predecessor);
        }

        let tree = self.completed.remove(root).expect("tree disappeared before emission");
        let inputs_fifo = self.required_inputs(&tree);
        let original_root = self.original.get_op(root);
        let leading_operand_is_folded = original_root.inputs_fifo.iter().take(2).any(|&input| {
            self.producers
                .get(input)
                .is_some_and(|&producer| self.original_to_tree.get(producer) == Some(&root))
        });
        let kind = match original_root.kind {
            // Folding either leading operand fixes its position inside the tree, so the virtual
            // operation can no longer exchange those operands when it is scheduled.
            OpNodeKind::Flippable(operation) if leading_operand_is_folded => {
                OpNodeKind::Normal(operation)
            }
            kind => kind,
        };
        let mut operation = self.builder.begin_op(kind);
        let new_id = operation.id();
        for predecessor in &predecessors {
            operation.add_predecessor(self.emitted[predecessor]);
        }
        for input in inputs_fifo {
            operation.add_input(self.original_to_value[input]);
        }
        let mut operation = operation.end_inputs_begin_outputs();
        for &output in original_root.outputs_fifo {
            let mapped = operation.add_output();
            self.original_to_value.insert_no_prev(output, mapped);
        }

        assert_eq!(self.trees.push(tree), new_id);
        self.emitted.insert_no_prev(root, new_id);
        assert!(self.emitting.remove(root));
        new_id
    }

    fn into_graph(mut self) -> TreeGraph {
        for root in std::mem::take(&mut self.materialization_order) {
            self.emit_tree(root);
        }

        let mut builder = self.builder.end_ops_begin_end_stack();
        for &original in self.original.output_values_fifo() {
            builder.push_end_stack_value(self.original_to_value[original]);
        }

        TreeGraph { graph: builder.finish(), flipped: self.flipped, trees: self.trees }
    }
}

fn collect_ancestors(original: &OpGraph) -> IndexVec<OpNodeId, DenseIndexSet<OpNodeId>> {
    let mut ancestors = IndexVec::with_capacity(original.total_ops() as usize);
    for operation in original.op_ids() {
        let mut set = DenseIndexSet::with_capacity_in_bits(original.total_ops() as usize);
        for predecessor in original.get_predecessors(operation).iter() {
            set.add(predecessor);
            set.union_with(&ancestors[predecessor]);
        }
        assert_eq!(ancestors.push(set), operation);
    }
    ancestors
}

#[cfg(test)]
mod tests;
