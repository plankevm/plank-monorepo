use crate::op_graph::{OpGraph, OpNodeId, OpNodeKind, ValueNodeId};
use plank_core::Idx;
use sir_data::StaticAllocId;

pub use crate::stack_ops::{ParsedStackOps, ShuffleConfig, StackOps, gas_cost, parse_stack_ops};

const MAX_STACK_LENGTH: usize = 1024;

#[derive(Debug, Clone)]
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

    #[track_caller]
    pub fn from_fifo(values: &[ValueNodeId]) -> Self {
        assert!(values.len() <= MAX_STACK_LENGTH, "stack overflow");
        let mut this = EvmStack::new();
        this.stack_raw[MAX_STACK_LENGTH - values.len()..].copy_from_slice(values);
        this.stack_len = values.len().try_into().unwrap();
        this
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

pub struct TrackedStack<Sink: FnMut(StackOps)> {
    start_alloc_id: StaticAllocId,
    ops_sink: Sink,
    spilled: Vec<ValueNodeId>,
    inner: EvmStack,
}

impl<Sink: FnMut(StackOps)> TrackedStack<Sink> {
    pub fn new_from_evm(
        start_alloc_id: StaticAllocId,
        ops_sink: Sink,
        inner: EvmStack,
        spilled_capacity: usize,
    ) -> Self {
        Self { start_alloc_id, ops_sink, spilled: Vec::with_capacity(spilled_capacity), inner }
    }

    pub(crate) fn underlying_spilled(&self) -> &[ValueNodeId] {
        &self.spilled
    }

    pub(crate) fn new_from_parts(
        start_alloc_id: StaticAllocId,
        ops_sink: Sink,
        stack_fifo: &[ValueNodeId],
        spilled: Vec<ValueNodeId>,
    ) -> Self {
        Self { start_alloc_id, ops_sink, spilled, inner: EvmStack::from_fifo(stack_fifo) }
    }

    fn emit(&mut self, op: StackOps) {
        (self.ops_sink)(op);
    }

    #[track_caller]
    pub fn pop(&mut self) {
        self.inner.pop().expect("nothing to pop");
        self.emit(StackOps::Pop);
    }

    pub fn clone_with<S2: FnMut(StackOps)>(&self, ops_sink: S2) -> TrackedStack<S2> {
        TrackedStack {
            start_alloc_id: self.start_alloc_id,
            ops_sink,
            spilled: self.spilled.clone(),
            inner: self.inner.clone(),
        }
    }

    #[track_caller]
    pub fn op(&mut self, graph: &OpGraph, op_id: OpNodeId, flipped: bool) {
        let op = graph.get_op(op_id);
        let stack_op = match op.kind {
            OpNodeKind::Flippable(op_idx) if flipped => StackOps::Flipped(op_idx),
            OpNodeKind::Flippable(op_idx) => StackOps::Op(op_idx),
            OpNodeKind::Normal(op_idx) => {
                assert!(!flipped);
                StackOps::Op(op_idx)
            }
            OpNodeKind::RetDestPush(op_idx) => {
                assert!(!flipped);
                StackOps::CallRetPush(op_idx)
            }
        };

        for _ in op.inputs_fifo {
            self.inner.pop().expect("missing input");
        }
        for &output in op.outputs_fifo.iter().rev() {
            self.inner.push(output);
        }
        self.emit(stack_op);
    }

    #[track_caller]
    pub fn swap(&mut self, depth: u8) {
        assert!(depth > 0);
        self.inner.swap_with_top(depth.into());
        self.emit(StackOps::Swap(depth));
    }

    pub fn dup(&mut self, depth: u8) {
        self.inner.duplicate(depth.into());
        self.emit(StackOps::Dup(depth));
    }

    pub fn get_spilled(&self, target: ValueNodeId) -> Option<StaticAllocId> {
        self.spilled.iter().rposition(|&value| value == target).map(|i| self.alloc_id(i))
    }

    #[track_caller]
    fn alloc_id(&self, i: usize) -> StaticAllocId {
        self.start_alloc_id + u32::try_from(i).expect("overflow")
    }

    #[track_caller]
    pub fn spill_top(&mut self) -> StaticAllocId {
        let target = self.inner.pop().expect("nothing to pop");
        let new_alloc_id = self.alloc_id(self.spilled.len());
        self.spilled.push(target);
        self.emit(StackOps::Store(new_alloc_id));
        new_alloc_id
    }

    #[track_caller]
    pub fn unspill(&mut self, target: ValueNodeId) {
        let alloc = self.get_spilled(target).expect("nothing spilled at alloc");
        self.inner.push(target);
        self.emit(StackOps::Load(alloc));
    }

    #[track_caller]
    pub fn load(&mut self, target: StaticAllocId) {
        let value = self.spilled[(target - self.start_alloc_id) as usize];
        self.inner.push(value);
        self.emit(StackOps::Load(target));
    }

    pub fn stack(&self) -> &EvmStack {
        &self.inner
    }

    pub fn into_next_alloc_id(self) -> StaticAllocId {
        self.start_alloc_id + u32::try_from(self.spilled.len()).expect("overflow")
    }
}

impl<Sink: FnMut(StackOps)> std::ops::Deref for TrackedStack<Sink> {
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
