use crate::op_graph::{
    BitsetWord, OpGraph, OpNodeId, OpNodeKind, StoredOpView, ValueArenaIdx, ValueNodeId,
};
use plank_core::{Idx, IndexVec};
use std::marker::PhantomData;

fn add_to_set(set: &mut Vec<BitsetWord>, id: OpNodeId) {
    let word_idx = id.get() / BitsetWord::BITS;
    let shift = id.get() % BitsetWord::BITS;
    if word_idx as usize >= set.len() {
        set.resize(word_idx as usize + 1, 0);
    }
    set[word_idx as usize] |= 1 << shift;
}

fn union_bitset(dst: &mut Vec<BitsetWord>, src: &[BitsetWord]) {
    if dst.len() < src.len() {
        dst.resize(src.len(), 0);
    }
    for (dst, &src) in dst.iter_mut().zip(src) {
        *dst |= src;
    }
}

fn copy_bitset(dst: &mut [BitsetWord], src: &[BitsetWord]) {
    assert!(src.len() <= dst.len());
    dst[..src.len()].copy_from_slice(src);
}

pub struct AddingGraphInputs;
pub struct AddingGraphOps {
    inputs_end: ValueNodeId,
}
pub struct AddingGraphEndStack {
    inputs_end: ValueNodeId,
    end_stack_fifo_start: ValueArenaIdx,
}

pub struct AddingInputs;
pub struct AddingOutputs;

struct OpGraphStorage {
    op_predecessors: IndexVec<OpNodeId, Vec<BitsetWord>>,
    operations: IndexVec<OpNodeId, StoredOpView>,
    values: IndexVec<ValueNodeId, (Option<OpNodeId>, Vec<BitsetWord>)>,
    values_arena: IndexVec<ValueArenaIdx, ValueNodeId>,
    estimated_ops: usize,
}

pub struct OpGraphBuilder<Phase> {
    storage: OpGraphStorage,
    phase: Phase,
}

#[must_use]
pub struct OpBuilder<'g, Phase> {
    graph: &'g mut OpGraphStorage,
    op: OpNodeId,
    _phase: PhantomData<Phase>,
}

impl OpGraphStorage {
    fn with_capacity(estimated_ops: usize, estimated_values: usize) -> Self {
        Self {
            op_predecessors: IndexVec::with_capacity(estimated_ops),
            operations: IndexVec::with_capacity(estimated_ops),
            values: IndexVec::with_capacity(estimated_values),
            values_arena: IndexVec::with_capacity(estimated_ops * 4),
            estimated_ops,
        }
    }

    fn estimated_words(&self) -> usize {
        self.operations.len().max(self.estimated_ops).div_ceil(BitsetWord::BITS as usize)
    }

    fn begin_op(&mut self, kind: OpNodeKind) -> OpBuilder<'_, AddingInputs> {
        let inputs_outputs_start = self.values_arena.len_idx();
        let op = self.operations.push(StoredOpView { inputs_outputs_start, input_count: 0, kind });
        let estimated_words = self.estimated_words();
        assert_eq!(self.op_predecessors.push(Vec::with_capacity(estimated_words)), op);

        OpBuilder { graph: self, op, _phase: PhantomData }
    }

    fn add_predecessor(&mut self, operation: OpNodeId, predecessor: OpNodeId) {
        assert!(predecessor < operation, "operation predecessor must have an earlier ID");
        let (earlier, current_and_later) = self.op_predecessors.split_at_mut(operation.idx());
        let current = current_and_later.first_mut().expect("operation predecessor set disappeared");
        add_to_set(current, predecessor);
        union_bitset(current, &earlier[predecessor.idx()]);
    }

    fn finish(self, inputs_end: ValueNodeId, end_stack_fifo_start: ValueArenaIdx) -> OpGraph {
        assert_eq!(self.operations.len_idx(), self.op_predecessors.len_idx());

        let total_ops = self.operations.len();
        let total_values = self.values.len();
        let words_per_set = total_ops.div_ceil(BitsetWord::BITS as usize);
        let mut bit_sets_arena = vec![0; words_per_set * (total_ops + total_values)];

        let mut offset = 0;
        for predecessors in self.op_predecessors.iter() {
            copy_bitset(&mut bit_sets_arena[offset..][..words_per_set], predecessors);
            offset += words_per_set;
        }

        for (_, consumers) in self.values.iter() {
            copy_bitset(&mut bit_sets_arena[offset..][..words_per_set], consumers);
            offset += words_per_set;
        }

        let producers = self.values.iter().map(|(producer, _)| *producer).collect();

        OpGraph {
            total_ops: total_ops.try_into().expect("overflow"),
            total_values: total_values.try_into().expect("overflow"),

            inputs_end,
            end_stack_fifo_start,

            values_arena: self.values_arena,
            operations: self.operations,
            producers,

            bit_sets_arena,
        }
    }
}

impl OpGraphBuilder<AddingGraphInputs> {
    pub fn with_capacity(estimated_ops: usize, estimated_values: usize) -> Self {
        Self {
            storage: OpGraphStorage::with_capacity(estimated_ops, estimated_values),
            phase: AddingGraphInputs,
        }
    }

    pub fn push_input_value(&mut self) -> ValueNodeId {
        self.storage.values.push((None, Vec::with_capacity(self.storage.estimated_words())))
    }

    pub fn end_inputs_begin_ops(self) -> OpGraphBuilder<AddingGraphOps> {
        let inputs_end = self.storage.values.len_idx();
        OpGraphBuilder { storage: self.storage, phase: AddingGraphOps { inputs_end } }
    }
}

impl OpGraphBuilder<AddingGraphOps> {
    pub fn begin_op(&mut self, kind: OpNodeKind) -> OpBuilder<'_, AddingInputs> {
        self.storage.begin_op(kind)
    }

    pub fn end_ops_begin_end_stack(self) -> OpGraphBuilder<AddingGraphEndStack> {
        let end_stack_fifo_start = self.storage.values_arena.len_idx();
        OpGraphBuilder {
            storage: self.storage,
            phase: AddingGraphEndStack { inputs_end: self.phase.inputs_end, end_stack_fifo_start },
        }
    }
}

impl OpGraphBuilder<AddingGraphEndStack> {
    pub fn push_end_stack_value(&mut self, value: ValueNodeId) {
        self.storage.values_arena.push(value);
    }

    pub fn finish(self) -> OpGraph {
        self.storage.finish(self.phase.inputs_end, self.phase.end_stack_fifo_start)
    }
}

impl<Phase> OpBuilder<'_, Phase> {
    pub fn id(&self) -> OpNodeId {
        self.op
    }

    pub fn add_predecessor(&mut self, predecessor: OpNodeId) {
        self.graph.add_predecessor(self.op, predecessor);
    }
}

impl<'g> OpBuilder<'g, AddingInputs> {
    pub fn add_input(&mut self, value: ValueNodeId) {
        self.graph.values_arena.push(value);
        self.graph.operations[self.op].input_count += 1;

        let source = {
            let (source, consumers) = &mut self.graph.values[value];
            add_to_set(consumers, self.op);
            *source
        };
        if let Some(source) = source {
            self.graph.add_predecessor(self.op, source);
        }
    }

    pub fn end_inputs_begin_outputs(self) -> OpBuilder<'g, AddingOutputs> {
        OpBuilder { graph: self.graph, op: self.op, _phase: PhantomData }
    }
}

impl OpBuilder<'_, AddingOutputs> {
    pub fn add_output(&mut self) -> ValueNodeId {
        let estimated_words = self.graph.estimated_words();
        let value = self.graph.values.push((Some(self.op), Vec::with_capacity(estimated_words)));
        self.graph.values_arena.push(value);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sir_data::OperationIdx;

    #[test]
    fn preserves_producers_and_complete_predecessors() {
        let mut graph = OpGraphBuilder::with_capacity(3, 4);
        let input = graph.push_input_value();
        let mut graph = graph.end_inputs_begin_ops();

        let (first, first_value) = {
            let operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            let id = operation.id();
            (id, operation.end_inputs_begin_outputs().add_output())
        };
        let (second, second_value) = {
            let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            operation.add_predecessor(first);
            operation.add_predecessor(first);
            let id = operation.id();
            (id, operation.end_inputs_begin_outputs().add_output())
        };
        let third = {
            let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            operation.add_input(second_value);
            operation.id()
        };
        let graph = graph.end_ops_begin_end_stack().finish();

        assert_eq!(graph.get_producer(input), None);
        assert_eq!(graph.get_producer(first_value), Some(first));
        assert_eq!(graph.get_producer(second_value), Some(second));
        assert_eq!(graph.get_predecessors(first).count_members(), 0);
        assert!(graph.get_predecessors(second).contains(first));
        assert!(graph.get_predecessors(third).contains(first));
        assert!(graph.get_predecessors(third).contains(second));
        assert_eq!(graph.get_predecessors(third).count_members(), 2);
    }

    #[test]
    #[should_panic(expected = "operation predecessor must have an earlier ID")]
    fn rejects_a_non_earlier_predecessor() {
        let graph = OpGraphBuilder::with_capacity(1, 0);
        let mut graph = graph.end_inputs_begin_ops();
        let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        operation.add_predecessor(operation.id());
    }
}
