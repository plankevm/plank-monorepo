use crate::{
    op_graph::{BitsetWord, OpGraph, OpSet, ValueNodeId},
    stack::StackOps,
};

pub struct ScheduleSearchState {
    pub complete: Box<[BitsetWord]>,
    pub executed: Box<[StackOps]>,
    pub executed_cost: u32,
    pub estimated_remaining_cost: u32,
    pub values: Box<[ValueNodeId]>,
    pub stack_end: usize,
}

impl ScheduleSearchState {
    pub fn start(graph: &OpGraph) -> Self {
        Self {
            complete: vec![0; graph.words_per_set() as usize].into(),
            executed: Box::new([]),
            executed_cost: 0,
            estimated_remaining_cost: 0,
            values: graph.input_values_fifo().iter().collect(),
            stack_end: graph.input_values_fifo().len() as usize,
        }
    }

    pub fn cost(&self) -> u32 {
        self.executed_cost + self.estimated_remaining_cost
    }

    pub fn complete(&self, total_ops: u32) -> OpSet<'_> {
        OpSet::new(&self.complete, total_ops)
    }

    pub fn stack_fifo(&self) -> &[ValueNodeId] {
        &self.values[..self.stack_end]
    }

    pub fn spilled(&self) -> &[ValueNodeId] {
        &self.values[self.stack_end..]
    }

    pub fn is_redundant(&self, other: &ScheduleSearchState) -> bool {
        self.complete == other.complete
            && self.values == other.values
            && self.stack_end == other.stack_end
    }
}
