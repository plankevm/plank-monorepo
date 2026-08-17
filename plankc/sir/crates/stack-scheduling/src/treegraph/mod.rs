//! Inspired by the "Treegraph-based Instruction Scheduling for Stack-based Virtual Machines" by J.
//! Park, J. Park, W. Song et. al.
//!
//! Adapted to take effects into account, as well as taking advantage of flippable operations.

use crate::op_graph::{
    OpGraph, OpGraphBuilder, OpNodeId, OpView, ValueNodeId, builder::AddingGraphOps,
};
use plank_core::{DenseIndexMap, IndexVec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeStep {
    pub operation: OpNodeId,
    pub flipped: bool,
}

#[derive(Debug, Clone)]
struct Tree {
    prefix: Vec<OpNodeId>,
}

#[derive(Debug)]
pub struct TreeGraph {
    pub graph: OpGraph,
    trees: IndexVec<OpNodeId, Tree>,
}

pub fn build_tree_graph(original: &OpGraph) -> TreeGraph {
    let total_values = original.total_values() as usize;
    let mut roots = DenseIndexMap::with_capacity(total_values);
    for op_id in original.op_ids() {
        let op = original.get_op(op_id);
        if op.outputs_fifo.len() >= 2 {
            continue;
        }
        for &input in op.inputs_fifo {
            roots.insert_no_prev(input, op_id);
        }
    }

    let mut original_to_tree = DenseIndexMap::with_capacity(total_values);
    let mut builder = OpGraphBuilder::with_capacity(original.total_ops() as usize, total_values);
    for bb_input in original.input_values_fifo() {
        original_to_tree.insert_no_prev(bb_input, builder.push_input_value());
    }

    TreeGraphBuilder {
        roots,
        trees: IndexVec::with_capacity(total_values),
        builder: builder.end_inputs_begin_ops(),
        original_to_tree,
    }
    .fold_ops(original)
    .into_graph(original)
}

struct TreeGraphBuilder {
    roots: DenseIndexMap<ValueNodeId, OpNodeId>,
    builder: OpGraphBuilder<AddingGraphOps>,
    trees: IndexVec<OpNodeId, Tree>,
    original_to_tree: DenseIndexMap<ValueNodeId, ValueNodeId>,
}

impl TreeGraphBuilder {
    fn fold_op(&mut self, id: OpNodeId, op: OpView<'_>) -> Result<Vec<OpNodeId>, ValueNodeId> {
        let mut dfs_prefix = Vec::new();
        for &input in op.inputs_fifo {}

        dfs_prefix.push(id);

        Ok(dfs_prefix)
    }

    fn fold_ops(mut self, original: &OpGraph) -> Self {
        for op_id in original.op_ids() {
            let op = original.get_op(op_id);

            // if self.trees.get(index)

            for &input in op.inputs_fifo {}
        }

        self
    }

    fn into_graph(self, original: &OpGraph) -> TreeGraph {
        let mut builder = self.builder.end_ops_begin_end_stack();
        for &original in original.output_values_fifo() {
            builder.push_end_stack_value(self.original_to_tree[original]);
        }

        TreeGraph { graph: builder.finish(), trees: self.trees }
    }
}

#[cfg(test)]
mod tests;
