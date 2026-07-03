use crate::{ScheduleConfig, op_graph::ValueNodeId};

#[derive(Debug, Clone, Copy)]
pub(crate) enum IntraOp {
    Swap(u8),
    Dup(u8),
    SpillTop,
    Unspill(ValueNodeId),
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub stack: Vec<ValueNodeId>,
    pub spilled: Vec<ValueNodeId>,
    pub cost_so_far: u32,
    pub todo: u32,
    pub estimated_remaining_cost: u32,
    pub ops: Vec<IntraOp>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TargetCtx<'a> {
    pub config: ScheduleConfig,
    pub target: &'a [ValueNodeId],
    pub last_uses: &'a [ValueNodeId],
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct StateKey {
    pub stack: Box<[ValueNodeId]>,
    pub spilled: Box<[ValueNodeId]>,
}

impl Candidate {
    pub fn complete(&self) -> bool {
        self.todo == 0
    }

    pub fn key(&self) -> StateKey {
        StateKey {
            stack: self.stack.clone().into_boxed_slice(),
            spilled: self.spilled.clone().into_boxed_slice(),
        }
    }

    pub fn expand(mut self, ctx: TargetCtx<'_>, mut emit: impl FnMut(Candidate)) {
        if self.complete() {
            emit(self);
            return;
        }
    }
}
