use hashbrown::{HashMap, HashSet};
use plank_core::{DenseIndexMap, IndexVec, newtype_index};

use crate::intra_instr_scheduling::{IntraInstrError, IntraInstrStrategy};

newtype_index! {
    pub struct ValueId;
    pub struct OpNodeIdx;
    pub struct ScheduleIdx;
}

struct OpNode {
    inputs: Vec<ValueId>,
    outputs: Vec<ValueId>,
}

pub struct OperationGraph {
    operations: IndexVec<OpNodeIdx, OpNode>,
    must_precede: DenseIndexMap<OpNodeIdx, HashSet<OpNodeIdx>>,
}

impl OperationGraph {
    pub fn op_count(&self) -> usize {
        self.operations.len()
    }

    pub fn op_indices(&self) -> impl Iterator<Item = OpNodeIdx> {
        self.operations.iter_idx()
    }

    pub fn inputs(&self, op: OpNodeIdx) -> &[ValueId] {
        &self.operations[op].inputs
    }

    pub fn outputs(&self, op: OpNodeIdx) -> &[ValueId] {
        &self.operations[op].outputs
    }

    pub fn must_precede(&self, op: OpNodeIdx) -> Option<&HashSet<OpNodeIdx>> {
        self.must_precede.get(op)
    }
}

#[derive(Default)]
pub struct OperationGraphBuilder {
    operations: IndexVec<OpNodeIdx, OpNode>,
    must_precede: DenseIndexMap<OpNodeIdx, HashSet<OpNodeIdx>>,
}

impl OperationGraphBuilder {
    pub fn new() -> Self {
        Self { operations: IndexVec::new(), must_precede: DenseIndexMap::new() }
    }

    pub fn add_op(&mut self, inputs: Vec<ValueId>, outputs: Vec<ValueId>) -> OpNodeIdx {
        self.operations.push(OpNode { inputs, outputs })
    }

    pub fn must_precede(&mut self, before: OpNodeIdx, after: OpNodeIdx) -> &mut Self {
        self.must_precede.get_or_insert_with(after, HashSet::new).insert(before);
        self
    }

    pub fn build(self) -> OperationGraph {
        OperationGraph { operations: self.operations, must_precede: self.must_precede }
    }
}

pub enum StackConfig {
    Flexible,
    Matching,
    FixedInput(Vec<ValueId>),
    FixedOutput(Vec<ValueId>),
    Fixed { input: Vec<ValueId>, output: Vec<ValueId> },
}

pub trait Scheduler {
    fn schedule(&self, graph: &OperationGraph, config: &StackConfig) -> Schedule;

    fn intra_instr_schedule(
        &self,
        current: &[ValueId],
        target: &[ValueId],
        spilled: Option<&HashMap<u32, ValueId>>,
        strategy: IntraInstrStrategy,
    ) -> Result<Vec<ScheduledOp>, IntraInstrError> {
        strategy.solve(current, target, spilled)
    }
}

pub struct Schedule {
    starting_stack: Vec<ValueId>,
    scheduled_ops: IndexVec<ScheduleIdx, ScheduledOp>,
}

impl Schedule {
    pub fn new(starting_stack: Vec<ValueId>, scheduled_ops: Vec<ScheduledOp>) -> Self {
        Self { starting_stack, scheduled_ops: IndexVec::from_vec(scheduled_ops) }
    }

    pub fn starting_stack(&self) -> &[ValueId] {
        &self.starting_stack
    }

    pub fn scheduled_ops(&self) -> impl Iterator<Item = (ScheduleIdx, &ScheduledOp)> {
        self.scheduled_ops.enumerate_idx()
    }
}

pub enum ScheduledOp {
    Op(OpNodeIdx),
    Swap(u8),
    Dup(u8),
    Pop,
    Spill { val: ValueId, offset: u32 },
    Load { val: ValueId, offset: u32 },
}
