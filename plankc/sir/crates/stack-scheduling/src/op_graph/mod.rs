use plank_core::{Idx, IndexVec, Span, newtype_index};
use sir_data::OperationIdx;

mod build_simple;
mod builder;

pub use build_simple::build_graph_simple;
pub use builder::OpGraphBuilder;

newtype_index! {
    pub struct OpNodeId;
    pub struct ValueNodeId;
    pub struct ValueArenaIdx;
}

#[derive(Debug, Clone, Copy)]
pub enum OpNodeKind {
    Flippable(OperationIdx),
    RetDestPush(OperationIdx),
    Normal(OperationIdx),
}

#[derive(Debug, Clone, Copy)]
struct StoredOpView {
    inputs_outputs_start: ValueArenaIdx,
    input_count: u32,
    kind: OpNodeKind,
}

pub type BitsetWord = u8;

#[derive(Debug)]
pub struct OpGraph {
    total_ops: u32,
    total_values: u32,

    inputs_end: ValueNodeId,
    end_stack_fifo_start: ValueArenaIdx,

    /// Holds `[(op_input*, op_output*)] ++ end_stack_fifo`
    values_arena: IndexVec<ValueArenaIdx, ValueNodeId>,
    operations: IndexVec<OpNodeId, StoredOpView>,

    /// Holds ` op_predecessors ++ value_consumers`
    bit_sets_arena: Vec<BitsetWord>,
}

#[derive(Debug, Clone, Copy)]
pub struct OpSet<'a> {
    words: &'a [BitsetWord],
}

#[derive(Debug)]
pub struct OpSetMut<'a> {
    words: &'a mut [BitsetWord],
}

impl<'a> From<&'a [BitsetWord]> for OpSet<'a> {
    fn from(words: &'a [BitsetWord]) -> Self {
        Self { words }
    }
}

impl<'a> From<&'a mut [BitsetWord]> for OpSetMut<'a> {
    fn from(words: &'a mut [BitsetWord]) -> Self {
        Self { words }
    }
}

impl OpGraph {
    pub fn total_ops(&self) -> u32 {
        self.total_ops
    }

    pub fn total_values(&self) -> u32 {
        self.total_values
    }

    pub fn op_ids(&self) -> impl Iterator<Item = OpNodeId> + '_ {
        self.operations.iter_idx()
    }

    pub fn input_values_fifo(&self) -> Span<ValueNodeId> {
        Span::new(ValueNodeId::ZERO, self.inputs_end)
    }

    pub fn is_input(&self, id: ValueNodeId) -> bool {
        id < self.inputs_end
    }

    pub fn output_values_fifo(&self) -> &[ValueNodeId] {
        &self.values_arena[self.end_stack_fifo_start..]
    }

    pub fn uses_remaining(&self, completed: &OpSet<'_>, value: ValueNodeId) -> u32 {
        let consumers = self.get_consumers(value);
        let total_uses = consumers.count_members();
        let already_used = consumers.intersect_count(completed);
        total_uses - already_used
    }

    fn get_bit_set(&self, bit_set_idx: usize) -> OpSet<'_> {
        let words_per_set = self.total_ops.div_ceil(BitsetWord::BITS) as usize;
        OpSet { words: &self.bit_sets_arena[words_per_set * bit_set_idx..][..words_per_set] }
    }

    pub fn get_predecessors(&self, id: OpNodeId) -> OpSet<'_> {
        self.get_bit_set(id.idx())
    }

    pub fn get_consumers(&self, id: ValueNodeId) -> OpSet<'_> {
        self.get_bit_set(self.total_ops as usize + id.idx())
    }

    pub fn get_op(&self, id: OpNodeId) -> OpView<'_> {
        let op = self.operations[id];
        let op_values_end = match self.operations.get(id + 1) {
            Some(stored_next) => stored_next.inputs_outputs_start,
            None => self.end_stack_fifo_start,
        };
        let op_values = &self.values_arena[Span::new(op.inputs_outputs_start, op_values_end)];

        OpView {
            inputs_fifo: &op_values[..op.input_count as usize],
            outputs_fifo: &op_values[op.input_count as usize..],
            predecessors: self.get_predecessors(id),
            kind: op.kind,
        }
    }
}

pub struct OpView<'g> {
    pub inputs_fifo: &'g [ValueNodeId],
    pub outputs_fifo: &'g [ValueNodeId],
    pub predecessors: OpSet<'g>,
    pub kind: OpNodeKind,
}

impl OpSet<'_> {
    pub fn count_members(&self) -> u32 {
        self.words.iter().copied().map(BitsetWord::count_ones).sum()
    }

    pub fn intersect_count(&self, other: &Self) -> u32 {
        self.words.iter().zip(other.words.iter()).map(|(&x, &y)| (x & y).count_ones()).sum()
    }

    pub fn is_super(&self, other: &Self) -> bool {
        self.words
            .iter()
            .zip(other.words.iter())
            .all(|(&super_limb, &sub_limb)| super_limb & sub_limb == sub_limb)
    }

    pub fn contains(&self, op: OpNodeId) -> bool {
        let i = op.const_get();
        let word_idx = i / BitsetWord::BITS;
        let word_shift = i % BitsetWord::BITS;
        self.words[word_idx as usize] & (1 << word_shift) != 0
    }
}

impl OpSetMut<'_> {
    pub fn as_set(&self) -> OpSet<'_> {
        OpSet { words: self.words }
    }

    pub fn contains(&self, op: OpNodeId) -> bool {
        self.as_set().contains(op)
    }

    pub fn flip(&mut self, op: OpNodeId) {
        let i = op.const_get();
        let word_idx = i / BitsetWord::BITS;
        let word_shift = i % BitsetWord::BITS;
        self.words[word_idx as usize] ^= 1 << word_shift;
    }
}
