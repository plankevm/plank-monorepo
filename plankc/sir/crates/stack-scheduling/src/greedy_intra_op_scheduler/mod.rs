use crate::{
    op_graph::{OpGraph, OpNodeId, OpSet, ValueNodeId},
    stack::{ScheduleConfig, StackOps, TrackedStack},
};
use plank_core::LoopLimit;

mod permute;

#[cfg(test)]
mod tests;

pub(crate) fn greedy_schedule_op<Sink: FnMut(StackOps)>(
    config: ScheduleConfig,
    stack: &mut TrackedStack<Sink>,
    graph: &OpGraph,
    op_id: OpNodeId,
    complete: OpSet<'_>,
) {
    let op = graph.get_op(op_id);

    let mut unique_last_uses_on_stack = Vec::with_capacity(op.inputs_fifo.len() / 2);
    for &value in op.inputs_fifo {
        if graph.is_last_use(complete, value)
            && !unique_last_uses_on_stack.contains(&value)
            && stack.count(value) > 0
        {
            unique_last_uses_on_stack.push(value);
        }
    }

    let head = unique_last_uses_on_stack.len().try_into().expect("overflow");

    let mut preparer = GreedyOperandPreparer::new(head, config, stack, op.inputs_fifo);

    let mut limit = LoopLimit::max(100_000);
    while !matches!(preparer.progress(), Status::Done) {
        limit.tick();
    }

    stack.op(graph, op_id, false);
}

enum Status {
    Done,
    ProcessNext,
}

struct GreedyOperandPreparer<'a, Sink: FnMut(StackOps)> {
    head: u16,
    max_dup_depth: u8,
    stack: &'a mut TrackedStack<Sink>,
    target: &'a [ValueNodeId],
}

impl<'a, Sink: FnMut(StackOps)> GreedyOperandPreparer<'a, Sink> {
    fn progress(&mut self) -> Status {
        if self.head == 0 {
            self.trivial_push_only();
            return Status::Done;
        }

        assert!(self.head <= self.stack.len());

        if usize::from(self.head) == self.target.len() {
            if self.stack.fifo().iter().zip(self.target).all(|(a, b)| a == b) {
                return Status::Done;
            }

            self.permute_allow_correct();
            return Status::ProcessNext;
        }

        let top = self.stack.fifo()[0];
        let target_top = self.index_target_aligned(0);

        if top != target_top && self.permute_never_undo_top() {
            return Status::ProcessNext;
        }
        self.dup_strategy();

        Status::ProcessNext
    }

    fn dup_strategy(&mut self) {
        todo!()
    }

    fn permute_allow_correct(&mut self) {
        todo!()
    }

    #[must_use]
    fn permute_never_undo_top(&mut self) -> bool {
        todo!()
    }

    fn trivial_push_only(&mut self) {
        for &value in self.target.iter().rev() {
            if self.try_push(value) {
                continue;
            }

            todo!("spill")
        }
    }

    fn new(
        head: u16,
        config: ScheduleConfig,
        stack: &'a mut TrackedStack<Sink>,
        target: &'a [ValueNodeId],
    ) -> Self {
        Self { head, max_dup_depth: config.max_dup_depth, stack, target }
    }

    #[must_use]
    fn try_push(&mut self, value: ValueNodeId) -> bool {
        if let Some(pos) =
            self.stack.find_first(value).filter(|&depth| depth <= self.max_dup_depth as u16)
        {
            self.stack.dup(pos as u8);
            return true;
        }

        if let Some(alloc) = self.stack.get_spilled(value) {
            self.stack.load(alloc);
            return true;
        }

        false
    }

    fn index_target_aligned(&self, i: usize) -> ValueNodeId {
        self.target[self.target_depth_delta() + i]
    }

    fn target_depth_delta(&self) -> usize {
        self.target.len() - usize::from(self.head)
    }
}
