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
use plank_core::{DenseIndexMap, DenseIndexSet, IndexVec, newtype_index};
use sir_data::OperationIdx;

newtype_index! {
    pub struct NewOpId;
    pub struct OriginalOpId;
}

impl From<OpNodeId> for NewOpId {
    fn from(value: OpNodeId) -> Self {
        Self::new(value.const_get())
    }
}

impl From<OpNodeId> for OriginalOpId {
    fn from(value: OpNodeId) -> Self {
        Self::new(value.const_get())
    }
}

impl Into<OpNodeId> for NewOpId {
    fn into(self) -> OpNodeId {
        OpNodeId::new(self.const_get())
    }
}

impl Into<OpNodeId> for OriginalOpId {
    fn into(self) -> OpNodeId {
        OpNodeId::new(self.const_get())
    }
}

pub fn collect_depth_first(
    original: &OpGraph,
    df_nodes: &mut Vec<OriginalOpId>,
    node: OriginalOpId,
) {
    if df_nodes.contains(&node) {
        return;
    }

    for pred in original.get_predecessors(node.into()).iter() {
        collect_depth_first(original, df_nodes, pred.into());
    }
    df_nodes.push(node);
}

pub fn build_tree_graph(original: &OpGraph) -> TreeGraph {
    let total_ops = original.total_ops() as usize;
    let total_values = original.total_values() as usize;
    let mut values_og_to_new = DenseIndexMap::with_capacity(total_values);

    let mut builder = OpGraphBuilder::with_capacity(total_ops, total_values);
    for input in original.input_values_fifo() {
        let new_input = builder.push_input_value();
        println!("{input} => {new_input}");
        values_og_to_new.insert(input, new_input);
    }
    let builder = builder.end_inputs_begin_ops();

    let mut builder = TreeGraphBuilder {
        total_ops,
        original,
        builder,
        values_og_to_new,
        ops: DenseIndexMap::with_capacity(total_ops),
        trees: IndexVec::with_capacity(total_ops),
    };

    let mut nodes = Vec::new();
    for id in original.op_ids() {
        collect_depth_first(original, &mut nodes, id.into());
    }
    println!("nodes: {:?}", nodes);

    for id in nodes {
        let id = OriginalOpId::from(id);
        if foldable_into_larger_tree(original, id) {
            continue;
        }
        if let Some(pending) = builder.try_fold(id) {
            println!("[top-level]");
            builder.materialize_pending(pending);
        }
    }

    let mut tg = builder.builder.end_ops_begin_end_stack();
    for &output in original.output_values_fifo() {
        tg.push_end_stack_value(builder.values_og_to_new[output]);
    }

    TreeGraph { graph: tg.finish(), flipped: DenseIndexSet::new(), trees: builder.trees }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeStep {
    pub operation: OriginalOpId,
    pub flipped: bool,
}

#[derive(Debug)]
pub struct TreeGraph {
    pub graph: OpGraph,
    flipped: DenseIndexSet<OriginalOpId>,
    trees: IndexVec<OperationIdx, Tree>,
}

#[derive(Debug, Clone, Copy)]
struct Tree {
    root: OriginalOpId,
    folded_count: u32,
}

impl TreeGraph {
    pub fn original_operations(&self, original: &OpGraph, operation: NewOpId) -> Vec<TreeStep> {
        let idx = match self.graph.op_kind(operation.into()) {
            OpNodeKind::Flippable(idx) | OpNodeKind::Normal(idx) => idx,
            OpNodeKind::RetDestPush(_) => unreachable!("treegraph doesn't add `RetDestPush`"),
        };
        let tree = self.trees[idx];

        let mut steps = Vec::new();

        fn iter_ops(
            steps: &mut Vec<TreeStep>,
            tg: &TreeGraph,
            original: &OpGraph,
            root: OriginalOpId,
            folded_count: u32,
        ) {
            let op = original.get_op(root.into());
            for &input in op.inputs_fifo[..folded_count as usize].iter().rev() {
                let producer = original.get_producer(input).expect("tree references bb input");
                iter_ops(steps, tg, original, producer.into(), original.op_input_count(producer))
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
        root: OriginalOpId,
        folded_count: u32,
        flipped: bool,
    ) {
        let op = original.get_op(root.into());
        for &input in op.inputs_fifo[..folded_count as usize].iter().rev() {
            let producer = original.get_producer(input).expect("tree member consumes input").into();
            self.expand_tree(
                original,
                ops,
                producer,
                original.op_input_count(producer.into()),
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

struct TreeGraphBuilder<'g> {
    total_ops: usize,
    original: &'g OpGraph,
    builder: OpGraphBuilder<AddingGraphOps>,
    values_og_to_new: DenseIndexMap<ValueNodeId, ValueNodeId>,
    ops: DenseIndexMap<OriginalOpId, NewOpId>,

    trees: IndexVec<OperationIdx, Tree>,
}

#[derive(Debug)]
struct PendingTree<'g> {
    members: DenseIndexSet<OriginalOpId>,
    root: OriginalOpId,
    og_view: OpView<'g>,
    folded_count: usize,
}

impl<'g> TreeGraphBuilder<'g> {
    fn tree_is_valid_successor(
        &self,
        predecessors: &DenseIndexSet<OriginalOpId>,
        tree_members: &DenseIndexSet<OriginalOpId>,
    ) -> bool {
        for member in tree_members.iter() {
            for potential_mid in predecessors.iter() {
                if self.original.get_predecessors(potential_mid.into()).contains(member.into()) {
                    return false;
                }
            }
        }

        true
    }

    fn materialize_pending(&mut self, pending: PendingTree<'_>) {
        println!("[MATERIALIZING] {pending:?}");
        let folded_count = pending.folded_count.try_into().expect("overflow");
        let virtual_op_idx = self.trees.push(Tree { root: pending.root, folded_count });

        let mut new_op = self.builder.begin_op(match pending.og_view.kind {
            OpNodeKind::Flippable(_) if folded_count == 0 => OpNodeKind::Flippable(virtual_op_idx),
            _ => OpNodeKind::Normal(virtual_op_idx),
        });
        self.ops.insert(pending.root, new_op.id().into());

        for &input in &pending.og_view.inputs_fifo[pending.folded_count..] {
            println!("input: {:?}", input);
            new_op.add_input(self.values_og_to_new[input]);
        }

        for pred in pending.og_view.predecessors.iter() {
            let pred = OriginalOpId::from(pred);
            new_op.add_predecessor(self.ops[pred].into());
        }

        let mut new_op = new_op.end_inputs_begin_outputs();
        for &output in pending.og_view.outputs_fifo {
            let new_output = new_op.add_output();
            println!("{output} => {new_output}");
            self.values_og_to_new.insert(output, new_output);
        }
    }

    fn try_fold(&mut self, root: OriginalOpId) -> Option<PendingTree<'g>> {
        if self.ops.contains(root) {
            return None;
        }

        let og_view = self.original.get_op(root.into());
        let mut members = DenseIndexSet::<OriginalOpId>::with_capacity_in_bits(self.total_ops);
        let mut predecessors = DenseIndexSet::<OriginalOpId>::with_capacity_in_bits(self.total_ops);
        let mut folded_count = 0;

        for pred in og_view.predecessors.iter() {
            predecessors.add(pred.into());
        }

        for &input in og_view.inputs_fifo {
            if self.values_og_to_new.contains(input) {
                break;
            }
            let producer = self.original.get_producer(input).expect("unmapped input").into();

            let Some(pending) = self.try_fold(producer) else { break };

            println!("predecessors: {:?}", predecessors);
            println!("pending.members: {:?}", pending.members);

            if !self.tree_is_valid_successor(&predecessors, &pending.members) {
                println!("[invalid succ]");
                self.materialize_pending(pending);
                break;
            }

            members.union_with(&pending.members);
            for pred in pending.og_view.predecessors.iter() {
                predecessors.add(OriginalOpId::from(pred));
            }

            folded_count += 1;
        }

        for &input in &og_view.inputs_fifo[folded_count..] {
            if self.values_og_to_new.contains(input) {
                continue;
            }

            let producer = self.original.get_producer(input).expect("unmapped input").into();
            if let Some(pending) = self.try_fold(producer) {
                println!("[post unfoldable succ]");
                self.materialize_pending(pending);
            }
        }

        members.add(root);
        let pending = PendingTree { members, root, og_view, folded_count };

        if folded_count == og_view.inputs_fifo.len()
            && let &[output] = og_view.outputs_fifo
            && !self.original.output_values_fifo().contains(&output)
            && let Ok(sole_consumer) = self.original.get_consumers(output).iter().exactly_one()
            && let consumer = self.original.get_op(sole_consumer)
            && consumer.inputs_fifo.iter().filter(|&&v| v == output).count() == 1
        {
            return Some(pending);
        }

        println!("[not usable as tree child]");
        self.materialize_pending(pending);

        None
    }
}

fn foldable_into_larger_tree(original: &OpGraph, id: OriginalOpId) -> bool {
    let op = original.get_op(id.into());
    let &[value] = op.outputs_fifo else { return false };
    let Ok(consumer) = original.get_consumers(value).iter().exactly_one() else { return false };
    original.get_op(consumer).inputs_fifo.iter().filter(|&&v| v == value).exactly_one().is_ok()
}

#[cfg(test)]
mod tests;
