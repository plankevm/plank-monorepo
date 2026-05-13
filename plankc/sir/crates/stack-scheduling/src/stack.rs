use std::cell::Cell;

use crate::op_graph::{OpGraph, OpNodeId, OpNodeKind, ValueNodeId};
use plank_core::list_of_lists::ListOfListsPusher;
use sir_data::{BasicBlockId, Idx, OperationIdx, StaticAllocId};

const MAX_STACK_LENGTH: usize = 1024;

#[derive(Debug, Clone, Copy)]
pub struct EvmStack {
    stack_raw: [ValueNodeId; MAX_STACK_LENGTH],
    stack_len: u16,
}

impl Default for EvmStack {
    fn default() -> Self {
        Self::new()
    }
}

impl EvmStack {
    pub const fn new() -> Self {
        Self { stack_raw: [ValueNodeId::ZERO; MAX_STACK_LENGTH], stack_len: 0 }
    }

    pub const fn is_empty(&self) -> bool {
        self.stack_len == 0
    }

    pub const fn len(&self) -> u16 {
        self.stack_len
    }

    pub const fn pop(&mut self) -> Option<ValueNodeId> {
        if self.stack_len == 0 {
            return None;
        }
        let value = self.stack_raw[MAX_STACK_LENGTH - self.stack_len as usize];
        self.stack_len -= 1;
        Some(value)
    }

    pub fn fifo(&self) -> &[ValueNodeId] {
        &self.stack_raw[MAX_STACK_LENGTH - self.stack_len as usize..]
    }

    fn stack_fifo_mut(&mut self) -> &mut [ValueNodeId] {
        &mut self.stack_raw[MAX_STACK_LENGTH - self.stack_len as usize..]
    }

    pub fn duplicate(&mut self, depth_index: u16) {
        let value = self.fifo()[depth_index as usize];
        self.push(value);
    }

    pub fn swap_with_top(&mut self, depth_index: u16) {
        self.exchange(0, depth_index);
    }

    pub fn exchange(&mut self, depth_index_n: u16, depth_index_m: u16) {
        self.stack_fifo_mut().swap(depth_index_n as usize, depth_index_m as usize);
    }

    pub fn get_by_depth(&self, depth_index: u16) -> Option<ValueNodeId> {
        self.fifo().get(depth_index as usize).copied()
    }

    pub const fn push(&mut self, value: ValueNodeId) {
        if self.stack_len as usize >= MAX_STACK_LENGTH {
            panic!(
                "stack overflow: your program uses *a lot* of locals, you are probably the first to encounter this issue, please open an issue on our github, we want to know what kind of contracts you're writing :D"
            );
        }
        self.stack_raw[MAX_STACK_LENGTH - self.stack_len as usize - 1] = value;
        self.stack_len += 1;
    }

    pub fn count(&self, target: ValueNodeId) -> u16 {
        self.fifo().iter().filter(|&&value| value == target).count() as u16
    }

    pub fn find_first(&self, target: ValueNodeId) -> Option<u16> {
        self.fifo().iter().position(|&value| value == target).map(|pos| pos as u16)
    }

    pub fn top(&self) -> Option<ValueNodeId> {
        self.fifo().first().copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackOps {
    Swap(u8),
    Dup(u8),
    Pop,
    Op(OperationIdx),
    CallRetPush(OperationIdx),
    Exchange(u8, u8),
    Store(StaticAllocId),
    Load(StaticAllocId),
}

impl StackOps {
    pub fn is_valid(self, config: ScheduleConfig) -> bool {
        match self {
            StackOps::Swap(depth) => depth <= config.max_swap_depth,
            StackOps::Dup(depth) => depth <= config.max_dup_depth,
            StackOps::Exchange(n, m) => {
                n.checked_add(m).is_some_and(|sum| sum <= config.max_exchange_range)
            }
            StackOps::Op(_)
            | StackOps::Pop
            | StackOps::Store(_)
            | StackOps::Load(_)
            | StackOps::CallRetPush(_) => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScheduleConfig {
    pub max_swap_depth: u8,
    pub max_dup_depth: u8,
    /// Given 0-indexed stack depths `m`, `n`, the `max_exchange_range` represents the constraints
    /// such that all valid `(m, n)` must satisfy `m + n <= max_exchange_range`
    pub max_exchange_range: u8,
    pub exchange_cost: u8,
}

impl Default for ScheduleConfig {
    fn default() -> Self {
        Self { max_swap_depth: 16, max_dup_depth: 15, max_exchange_range: 16, exchange_cost: 9 }
    }
}

pub struct TrackedStack<'ir, 'sink, 'list> {
    next_alloc_id: &'ir Cell<StaticAllocId>,
    ops_sink: &'sink mut ListOfListsPusher<'list, BasicBlockId, StackOps>,
    spilled: Vec<(ValueNodeId, StaticAllocId)>,
    inner: EvmStack,
}

impl<'ir, 'sink, 'list> TrackedStack<'ir, 'sink, 'list> {
    pub fn new_from_evm(
        next_alloc_id: &'ir Cell<StaticAllocId>,
        ops_sink: &'sink mut ListOfListsPusher<'list, BasicBlockId, StackOps>,
        inner: EvmStack,
        spilled_capacity: usize,
    ) -> Self {
        Self { next_alloc_id, ops_sink, spilled: Vec::with_capacity(spilled_capacity), inner }
    }

    #[track_caller]
    pub fn pop(&mut self) {
        self.inner.pop().expect("nothing to pop");
        self.ops_sink.push(StackOps::Pop);
    }

    #[track_caller]
    pub fn op(&mut self, graph: &OpGraph, op_id: OpNodeId) {
        let op = &graph.operations[op_id];
        let (stack_op, flippable) = match op.kind {
            OpNodeKind::Flippable(op_idx) => (StackOps::Op(op_idx), true),
            OpNodeKind::Normal(op_idx) => (StackOps::Op(op_idx), false),
            OpNodeKind::RetDestPush(op_idx) => (StackOps::CallRetPush(op_idx), false),
        };
        let mut flipping = false;
        for (i, &target) in (0usize..).zip(&op.consumes_fifo) {
            let actual = self.inner.pop().expect("missing input");

            let correct = if flippable && i == 0 && actual == op.consumes_fifo[1] {
                flipping = true;
                true
            } else if flippable && flipping && i == 1 {
                actual == op.consumes_fifo[0]
            } else {
                target == actual
            };
            assert!(correct, "incorrect op schedule");
        }
        self.ops_sink.push(stack_op);
        for &output in graph.operations[op_id].produces_fifo.iter().rev() {
            self.inner.push(output);
        }
    }

    pub fn dup(&mut self, depth: u8) {
        self.inner.duplicate(depth.into());
        self.ops_sink.push(StackOps::Dup(depth));
    }

    pub fn get_spilled(&self, target: ValueNodeId) -> Option<StaticAllocId> {
        self.spilled.iter().rev().find_map(|&(value, alloc)| (value == target).then_some(alloc))
    }

    #[track_caller]
    pub fn spill_top(&mut self) -> StaticAllocId {
        let target = self.inner.pop().expect("nothing to pop");
        let new_alloc_id = self.next_alloc_id.get();
        self.next_alloc_id.set(new_alloc_id + 1);
        self.ops_sink.push(StackOps::Store(new_alloc_id));
        self.spilled.push((target, new_alloc_id));
        new_alloc_id
    }

    #[track_caller]
    pub fn unspill(&mut self, target: ValueNodeId) {
        let alloc = self.get_spilled(target).expect("nothing spilled at alloc");
        self.inner.push(target);
        self.ops_sink.push(StackOps::Load(alloc));
    }

    #[track_caller]
    pub fn load(&mut self, target: StaticAllocId) {
        let &(value, _) = self
            .spilled
            .iter()
            .find(|&&(_, alloc)| alloc == target)
            .expect("nothing spilled at alloc");
        self.inner.push(value);
        self.ops_sink.push(StackOps::Load(target));
    }

    pub fn stack(&self) -> &EvmStack {
        &self.inner
    }
}

impl std::ops::Deref for TrackedStack<'_, '_, '_> {
    type Target = EvmStack;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_new() {
        let stack = EvmStack::new();
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn basic_push_pop() {
        let mut stack = EvmStack::new();
        assert_eq!(stack.len(), 0);

        stack.push(ValueNodeId::new(0));
        stack.push(ValueNodeId::new(1));
        stack.push(ValueNodeId::new(2));
        assert_eq!(stack.len(), 3);

        assert_eq!(stack.pop(), Some(ValueNodeId::new(2)));
        assert_eq!(stack.pop(), Some(ValueNodeId::new(1)));
        assert_eq!(stack.pop(), Some(ValueNodeId::new(0)));
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn basic_find_first() {
        let mut stack = EvmStack::new();

        stack.push(ValueNodeId::new(0));
        stack.push(ValueNodeId::new(1));
        stack.push(ValueNodeId::new(2));
        assert_eq!(stack.len(), 3);

        assert_eq!(stack.find_first(ValueNodeId::new(2)), Some(0));
        assert_eq!(stack.find_first(ValueNodeId::new(1)), Some(1));
        assert_eq!(stack.find_first(ValueNodeId::new(0)), Some(2));
        assert_eq!(stack.find_first(ValueNodeId::new(4)), None);
    }
}
