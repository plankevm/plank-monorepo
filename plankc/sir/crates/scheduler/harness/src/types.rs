use hashbrown::HashSet;
use plank_core::{DenseIndexMap, newtype_index};

newtype_index! {
    pub struct ValueId;
    pub struct OpNodeIdx;
}

struct OpNode {
    inputs: Vec<ValueId>,
    outputs: Vec<ValueId>,
}

pub struct OperationGraph {
    operations: DenseIndexMap<OpNodeIdx, OpNode>,
    must_follow: DenseIndexMap<OpNodeIdx, HashSet<OpNodeIdx>>,
}

pub enum StackConfig {
    Flexible,
    Matching(Vec<ValueId>),
    FixedInput(Vec<ValueId>),
    FixedOutput(Vec<ValueId>),
    Fixed { input: Vec<ValueId>, output: Vec<ValueId> },
}

pub trait Scheduler {
    fn schedule(&self, graph: &OperationGraph, config: &StackConfig) -> Schedule;

    fn intra_instr_schedule(&self, current: &[ValueId], target: &[ValueId]) -> Vec<ScheduledOp> {
        todo!()
    }
}

pub struct Schedule {
    starting_stack: Vec<ValueId>,
    scheduled_ops: Vec<ScheduledOp>,
}

impl Schedule {
    pub fn scheduled_ops(&self) -> impl Iterator<Item = &ScheduledOp> {
        self.scheduled_ops.iter()
    }
}

pub enum ScheduledOp {
    Op(OpNodeIdx),
    Swap(u8),
    Dup(u8),
    Pop,
    Spill(ValueId),
    Unspill(ValueId),
}
