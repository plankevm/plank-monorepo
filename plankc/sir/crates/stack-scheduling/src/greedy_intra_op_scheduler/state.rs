use crate::{op_graph::ValueNodeId, stack::StackOps};

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub stack: Vec<ValueNodeId>,
    pub cost_so_far: u32,
    pub todo: u32,
    pub ops: Vec<StackOps>,
}
