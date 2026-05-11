use crate::op_graph::{OpGraph, OpNodeId, ValueNodeId};
use sir_data::{Idx, OperationIdx};

const MAX_STACK_LENGTH: usize = 1024;

#[derive(Debug, Clone, Copy)]
pub struct VirtualStack {
    stack_raw: [ValueNodeId; MAX_STACK_LENGTH],
    stack_len: u16,
}

impl Default for VirtualStack {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualStack {
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
    Op(OpNodeId),
    CallRetPush(OperationIdx),
    Exchange(u8, u8),
    Store(u32),
    Load(u32),
}

impl StackOps {
    fn is_valid(self, config: ScheduleConfig) -> bool {
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
        Self { max_swap_depth: 17, max_dup_depth: 16, max_exchange_range: 17, exchange_cost: 9 }
    }
}

pub struct TrackedStack {
    spilled: Vec<Option<ValueNodeId>>,
    ops: Vec<StackOps>,
    inner: VirtualStack,
}

impl TrackedStack {
    pub fn new_from_vstack(
        inner: VirtualStack,
        ops_capacity: usize,
        spilled_capacity: usize,
    ) -> Self {
        Self {
            ops: Vec::with_capacity(ops_capacity),
            spilled: Vec::with_capacity(spilled_capacity),
            inner,
        }
    }

    pub fn into_ops(self) -> Vec<StackOps> {
        self.ops
    }

    #[track_caller]
    pub fn pop(&mut self) {
        self.inner.pop().expect("nothing to pop");
        self.ops.push(StackOps::Pop);
    }

    pub fn spilled(&self) -> impl Iterator<Item = (u32, ValueNodeId)> {
        (0..).zip(&self.spilled).filter_map(|(i, &value)| Some((i, value?)))
    }

    #[track_caller]
    pub fn op(&mut self, graph: &OpGraph, op: OpNodeId) {
        for &target in &graph.operations[op].consumes_fifo {
            let actual = self.inner.pop().expect("missing input");
            assert_eq!(target, actual, "incorrect op schedule");
        }
        self.ops.push(StackOps::Op(op));
        for &output in graph.operations[op].produces_fifo.iter().rev() {
            self.inner.push(output);
        }
    }

    pub fn dup(&mut self, depth: u8) {
        self.inner.duplicate(depth.into());
        self.ops.push(StackOps::Dup(depth));
    }

    #[track_caller]
    pub fn store(&mut self, slot: u32) {
        let value = self.inner.pop().expect("nothing to pop");
        if self.spilled.len() <= slot as usize {
            self.spilled.resize(slot as usize + 1, None);
        }
        self.spilled[slot as usize] = Some(value);

        self.ops.push(StackOps::Store(slot));
    }

    #[track_caller]
    pub fn load(&mut self, slot: u32) {
        let value = self.spilled[slot as usize].expect("nothing spilled at slot");
        self.inner.push(value);

        self.ops.push(StackOps::Load(slot));
    }

    pub fn stack(&self) -> &VirtualStack {
        &self.inner
    }
}

impl std::ops::Deref for TrackedStack {
    type Target = VirtualStack;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_new() {
        let stack = VirtualStack::new();
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn basic_push_pop() {
        let mut stack = VirtualStack::new();
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
        let mut stack = VirtualStack::new();

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
