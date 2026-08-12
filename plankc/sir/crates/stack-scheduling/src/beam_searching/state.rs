use crate::{
    ShuffleConfig,
    beam_searching::ScheduleConfig,
    op_graph::{BitsetWord, OpGraph, OpSet, OpSetMut, ValueNodeId},
    stack::StackOps,
};

pub struct ScheduleSearchState {
    pub(crate) complete: Box<[BitsetWord]>,
    pub(crate) executed: Box<[StackOps]>,
    pub(crate) executed_cost: u32,
    pub(crate) values: Box<[ValueNodeId]>,
    pub(crate) stack_end: usize,
}

impl ScheduleSearchState {
    pub fn start(graph: &OpGraph) -> Self {
        Self {
            complete: vec![0; graph.words_per_set() as usize].into(),
            executed: Box::new([]),
            executed_cost: 0,
            values: graph.input_values_fifo().iter().collect(),
            stack_end: graph.input_values_fifo().len() as usize,
        }
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
