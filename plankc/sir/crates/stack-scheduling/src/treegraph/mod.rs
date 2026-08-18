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
use plank_core::{DenseIndexMap, DenseIndexSet, IndexVec, newtype_index};

newtype_index! {
    struct TreeId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeStep {
    pub operation: OpNodeId,
    pub flipped: bool,
}

#[derive(Debug, Clone)]
struct Tree {
    steps: Vec<OpNodeId>,
}

#[derive(Debug)]
pub struct TreeGraph {
    pub graph: OpGraph,
    flipped: DenseIndexSet<OpNodeId>,
    trees: IndexVec<OpNodeId, Tree>,
}

impl TreeGraph {
    pub fn original_operations(&self, operation: OpNodeId) -> impl Iterator<Item = TreeStep> {
        self.trees[operation]
            .steps
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
            let root = *self.trees[tree_operation].steps.last().expect("empty tree");
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

    for op_id in original.op_ids() {
        let op = original.get_op(op_id);
        for &output in op.outputs_fifo {
            producers.insert_no_prev(output, op_id);
        }
        for &input in op.inputs_fifo {
            *use_counts.get_or_insert_with(input, || 0u32) += 1;
        }
    }

    let mut roots = DenseIndexMap::with_capacity(total_values);
    let mut operand_operations = DenseIndexSet::with_capacity_in_bits(total_ops);
    for consumer in original.op_ids() {
        for &input in original.get_op(consumer).inputs_fifo {
            let Some(&producer) = producers.get(input) else {
                continue;
            };
            if original.get_op(producer).outputs_fifo.len() == 1
                && use_counts.get(input) == Some(&1)
                && !original.output_values_fifo().contains(&input)
            {
                roots.insert_no_prev(input, consumer);
                assert!(operand_operations.add(producer));
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
        roots,
        producers,
        operand_operations,
        completed: IndexVec::with_capacity(total_ops),
        original_to_tree: DenseIndexMap::with_capacity(total_ops),
        ancestors: collect_ancestors(original),
        builder: builder.end_inputs_begin_ops(),
        original_to_value,
        trees: IndexVec::with_capacity(total_ops),
        flipped: DenseIndexSet::with_capacity_in_bits(total_ops),
        emitted: IndexVec::new(),
        emitting: DenseIndexSet::with_capacity_in_bits(total_ops),
    }
    .fold_ops()
    .into_graph()
}

#[derive(Debug, Clone)]
struct TreeEffects {
    must_be_before: DenseIndexSet<OpNodeId>,
}

#[derive(Debug, Clone)]
struct PendingTree {
    root: OpNodeId,
    steps: Vec<OpNodeId>,
    flipped: DenseIndexSet<OpNodeId>,
    operations: DenseIndexSet<OpNodeId>,
    inputs_fifo: Option<Vec<ValueNodeId>>,
    effects: TreeEffects,
}

enum FoldResult {
    Operand(PendingTree),
    Materialized,
}

#[derive(Default)]
struct TreeBuffers {
    pending: Vec<PendingTree>,
    accumulator: Vec<PendingTree>,
}

struct TreePlan {
    tree: PendingTree,
    accepted_operands: DenseIndexSet<OpNodeId>,
}

struct TreeGraphBuilder<'graph> {
    original: &'graph OpGraph,
    roots: DenseIndexMap<ValueNodeId, OpNodeId>,
    producers: DenseIndexMap<ValueNodeId, OpNodeId>,
    operand_operations: DenseIndexSet<OpNodeId>,
    completed: IndexVec<TreeId, PendingTree>,
    original_to_tree: DenseIndexMap<OpNodeId, TreeId>,
    ancestors: IndexVec<OpNodeId, DenseIndexSet<OpNodeId>>,
    builder: OpGraphBuilder<AddingGraphOps>,
    original_to_value: DenseIndexMap<ValueNodeId, ValueNodeId>,
    trees: IndexVec<OpNodeId, Tree>,
    flipped: DenseIndexSet<OpNodeId>,
    emitted: IndexVec<TreeId, Option<OpNodeId>>,
    emitting: DenseIndexSet<TreeId>,
}

impl TreeGraphBuilder<'_> {
    fn fold_op(&mut self, id: OpNodeId, as_operand: bool) -> FoldResult {
        let op = self.original.get_op(id);
        let mut operands = Vec::with_capacity(op.inputs_fifo.len());
        for &input in op.inputs_fifo {
            let candidate = if self.roots.get(input) == Some(&id) {
                let producer = self.producers[input];
                match self.fold_op(producer, true) {
                    FoldResult::Operand(candidate) => Some(candidate),
                    FoldResult::Materialized => unreachable!("foldable operand was materialized"),
                }
            } else {
                None
            };
            operands.push((input, candidate));
        }

        let normal = self.plan_tree(id, &operands, false);
        let plan = if matches!(op.kind, OpNodeKind::Flippable(_)) && op.inputs_fifo.len() >= 2 {
            let flipped = self.plan_tree(id, &operands, true);
            if flipped.tree.steps.len() > normal.tree.steps.len() { flipped } else { normal }
        } else {
            normal
        };

        for (_, operand) in operands {
            let Some(operand) = operand else {
                continue;
            };
            if !plan.accepted_operands.contains(operand.root) {
                self.materialize(operand);
            }
        }

        if as_operand {
            FoldResult::Operand(plan.tree)
        } else {
            self.materialize(plan.tree);
            FoldResult::Materialized
        }
    }

    fn plan_tree(
        &self,
        root: OpNodeId,
        operands: &[(ValueNodeId, Option<PendingTree>)],
        flipped: bool,
    ) -> TreePlan {
        let mut buffers = TreeBuffers::default();
        let operand_count = operands.len();
        let order = (0..operand_count)
            .rev()
            .map(|position| if flipped && position < 2 { 1 - position } else { position });

        for position in order {
            let Some(operand) = operands[position].1.clone() else {
                buffers.accumulator.clear();
                continue;
            };

            buffers.pending.clear();
            buffers.pending.push(operand);
            let proposed = self.join_trees(root, &buffers.accumulator, &buffers.pending, None);
            if self.is_valid_tree(&proposed) {
                buffers.accumulator.append(&mut buffers.pending);
            } else {
                buffers.accumulator.clear();
                std::mem::swap(&mut buffers.accumulator, &mut buffers.pending);
            }
        }

        while !buffers.accumulator.is_empty() {
            let proposed = self.join_trees(root, &[], &buffers.accumulator, Some(flipped));
            if self.is_valid_tree(&proposed) {
                break;
            }
            buffers.accumulator.remove(0);
        }

        let tree = self.join_trees(root, &[], &buffers.accumulator, Some(flipped));
        assert!(self.is_valid_tree(&tree));
        let mut accepted_operands = DenseIndexSet::new();
        for operand in &buffers.accumulator {
            assert!(accepted_operands.add(operand.root));
        }
        TreePlan { tree, accepted_operands }
    }

    fn join_trees(
        &self,
        root: OpNodeId,
        first: &[PendingTree],
        second: &[PendingTree],
        root_flipped: Option<bool>,
    ) -> PendingTree {
        let mut steps = Vec::new();
        let mut flipped = DenseIndexSet::with_capacity_in_bits(self.original.total_ops() as usize);
        for tree in first.iter().chain(second) {
            steps.extend_from_slice(&tree.steps);
            flipped.union_with(&tree.flipped);
        }
        if let Some(root_flipped) = root_flipped {
            steps.push(root);
            if root_flipped {
                assert!(flipped.add(root));
            }
        }
        self.describe_tree(root, steps, flipped)
    }

    fn describe_tree(
        &self,
        root: OpNodeId,
        steps: Vec<OpNodeId>,
        flipped: DenseIndexSet<OpNodeId>,
    ) -> PendingTree {
        let mut operations =
            DenseIndexSet::with_capacity_in_bits(self.original.total_ops() as usize);
        for &step in &steps {
            assert!(operations.add(step));
        }

        let mut must_be_before =
            DenseIndexSet::with_capacity_in_bits(self.original.total_ops() as usize);
        for operation in &operations {
            for predecessor in self.original.get_predecessors(operation).iter() {
                if !operations.contains(predecessor) {
                    must_be_before.add(predecessor);
                    must_be_before.union_with(&self.ancestors[predecessor]);
                }
            }
        }

        let inputs_fifo = self.required_inputs(&steps, &flipped);
        PendingTree {
            root,
            steps,
            flipped,
            operations,
            inputs_fifo,
            effects: TreeEffects { must_be_before },
        }
    }

    fn is_valid_tree(&self, tree: &PendingTree) -> bool {
        if tree.inputs_fifo.is_none() {
            return false;
        }

        let mut seen = DenseIndexSet::with_capacity_in_bits(self.original.total_ops() as usize);
        for &step in &tree.steps {
            for predecessor in self.original.get_predecessors(step).iter() {
                if tree.operations.contains(predecessor) && !seen.contains(predecessor) {
                    return false;
                }
            }
            seen.add(step);
        }

        !tree.effects.must_be_before.iter().any(|predecessor| tree.operations.contains(predecessor))
    }

    fn required_inputs(
        &self,
        steps: &[OpNodeId],
        flipped: &DenseIndexSet<OpNodeId>,
    ) -> Option<Vec<ValueNodeId>> {
        let mut stack = Vec::new();
        let mut inputs_fifo = Vec::new();
        for &step in steps {
            let op = self.original.get_op(step);
            let mut inputs = op.inputs_fifo.to_vec();
            if flipped.contains(step) {
                inputs.swap(0, 1);
            }
            for input in inputs {
                match stack.last() {
                    Some(&actual) if actual == input => {
                        stack.pop();
                    }
                    Some(_) => return None,
                    None => inputs_fifo.push(input),
                }
            }
            stack.extend(op.outputs_fifo.iter().rev().copied());
        }
        Some(inputs_fifo)
    }

    fn materialize(&mut self, tree: PendingTree) -> TreeId {
        let operations = tree.operations.clone();
        self.flipped.union_with(&tree.flipped);
        let tree_id = self.completed.push(tree);
        self.emitted.push(None);
        for operation in &operations {
            self.original_to_tree.insert_no_prev(operation, tree_id);
        }
        tree_id
    }

    fn fold_ops(mut self) -> Self {
        for op_id in self.original.op_ids() {
            if self.operand_operations.contains(op_id) {
                continue;
            }
            let FoldResult::Materialized = self.fold_op(op_id, false) else {
                unreachable!("tree root remained an operand")
            };
        }
        assert_eq!(self.original_to_tree.iter().count(), self.original.total_ops() as usize);
        self
    }

    fn emit_tree(&mut self, tree_id: TreeId) -> OpNodeId {
        if let Some(operation) = self.emitted[tree_id] {
            return operation;
        }
        assert!(self.emitting.add(tree_id), "cycle in tree graph");

        let mut predecessors = DenseIndexSet::with_capacity_in_bits(self.completed.len());
        for operation in &self.completed[tree_id].operations {
            for predecessor in self.original.get_predecessors(operation).iter() {
                let predecessor_tree = self.original_to_tree[predecessor];
                if predecessor_tree != tree_id {
                    predecessors.add(predecessor_tree);
                }
            }
        }
        for predecessor in &predecessors {
            self.emit_tree(predecessor);
        }

        let tree = self.completed[tree_id].clone();
        let root = self.original.get_op(tree.root);
        let leading_operand_is_folded = root.inputs_fifo.iter().take(2).any(|&input| {
            self.producers.get(input).is_some_and(|&producer| tree.operations.contains(producer))
        });
        let kind = match root.kind {
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
            operation.add_predecessor(self.emitted[predecessor].unwrap());
        }
        for input in tree.inputs_fifo.expect("only valid trees are materialized") {
            operation.add_input(self.original_to_value[input]);
        }
        let mut operation = operation.end_inputs_begin_outputs();
        for &output in root.outputs_fifo {
            let mapped = operation.add_output();
            self.original_to_value.insert_no_prev(output, mapped);
        }

        assert_eq!(self.trees.push(Tree { steps: tree.steps }), new_id);
        self.emitted[tree_id] = Some(new_id);
        assert!(self.emitting.remove(tree_id));
        new_id
    }

    fn into_graph(mut self) -> TreeGraph {
        for tree_id in self.completed.iter_idx().collect::<Vec<_>>() {
            self.emit_tree(tree_id);
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
