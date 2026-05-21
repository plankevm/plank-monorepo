use crate::op_graph_builder::{OpNodeId, ValueNodeId};
use plank_core::{Idx, IndexVec, Span, newtype_index};

newtype_index! {
    struct ValueArenaIdx;
}

#[derive(Debug, Clone, Copy)]
struct StoredOpView {
    inputs_output_start: ValueArenaIdx,
    inputs: u32,
}

pub type BitmapWord = u8;

#[derive(Debug)]
pub struct OpGraphView {
    total_ops: u32,
    total_values: u32,

    inputs_end: ValueNodeId,
    end_stack_fifo_end: ValueArenaIdx,

    /// Holds `end_stack_fifo ++ [(op_inputs, op_outputs)]`
    values_arena: IndexVec<ValueArenaIdx, ValueNodeId>,
    operations: IndexVec<OpNodeId, StoredOpView>,

    /// Holds `[op_predecessors] ++ [value_consumers]`
    bit_sets_arena: Vec<BitmapWord>,
}

#[derive(Debug)]
#[repr(transparent)]
pub struct OpSet([BitmapWord]);

impl<'a> From<&'a [BitmapWord]> for &'a OpSet {
    fn from(value: &'a [BitmapWord]) -> Self {
        unsafe { &*(value as *const [BitmapWord] as *const Self) }
    }
}

impl<'a> From<&'a mut [BitmapWord]> for &'a mut OpSet {
    fn from(value: &'a mut [BitmapWord]) -> Self {
        unsafe { &mut *(value as *mut [BitmapWord] as *mut Self) }
    }
}

impl OpGraphView {
    pub fn input_values_fifo(&self) -> Span<ValueNodeId> {
        Span::new(ValueNodeId::ZERO, self.inputs_end)
    }

    pub fn output_values_fifo(&self) -> &[ValueNodeId] {
        &self.values_arena[Span::new(ValueArenaIdx::ZERO, self.end_stack_fifo_end)]
    }

    pub fn uses_remaining(&self, completed: &OpSet, value: ValueNodeId) -> u32 {
        let consumers = self.get_consumers(value);
        let total_uses = consumers.count_members();
        let already_used = consumers.intersect_count(completed);
        total_uses - already_used
    }

    pub fn get_predecessors(&self, id: OpNodeId) -> &OpSet {
        let words_per_op = self.total_ops.div_ceil(BitmapWord::BITS);
        let start_offset = id.const_get() * words_per_op;
        (&self.bit_sets_arena[start_offset as usize..][..words_per_op as usize]).into()
    }

    pub fn get_consumers(&self, id: ValueNodeId) -> &OpSet {
        let words_per_op_or_value = self.total_ops.div_ceil(BitmapWord::BITS);
        let value_words_start = self.total_ops * words_per_op_or_value;

        let start_offset = value_words_start + id.const_get() * words_per_op_or_value;

        (&self.bit_sets_arena[start_offset as usize..][..words_per_op_or_value as usize]).into()
    }

    pub fn get_op(&self, id: OpNodeId) -> OpView<'_> {
        let op = self.operations[id];
        let op_values_end = match self.operations.get(id + 1) {
            Some(stored_next) => stored_next.inputs_output_start,
            None => self.values_arena.len_idx(),
        };
        let op_values = &self.values_arena[Span::new(op.inputs_output_start, op_values_end)];

        OpView {
            inputs_fifo: &op_values[..op.inputs as usize],
            outputs_fifo: &op_values[op.inputs as usize..],
            predecessors: self.get_predecessors(id),
        }
    }
}

pub struct OpView<'g> {
    pub inputs_fifo: &'g [ValueNodeId],
    pub outputs_fifo: &'g [ValueNodeId],
    pub predecessors: &'g OpSet,
}

impl OpSet {
    pub fn contains(&self, op: OpNodeId) -> bool {
        let i = op.const_get();
        let word_idx = i / BitmapWord::BITS;
        let word_shift = i % BitmapWord::BITS;
        self.0[word_idx as usize] & (1 << word_shift) != 0
    }

    pub fn count_members(&self) -> u32 {
        self.0.iter().copied().map(BitmapWord::count_ones).sum()
    }

    pub fn intersect_count(&self, other: &Self) -> u32 {
        self.0.iter().zip(other.0.iter()).map(|(&x, &y)| (x & y).count_ones()).sum()
    }

    pub fn is_super(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .all(|(&super_limb, &sub_limb)| super_limb & sub_limb == sub_limb)
    }

    pub fn flip(&mut self, op: OpNodeId) {
        let i = op.const_get();
        let word_idx = i / BitmapWord::BITS;
        let word_shift = i % BitmapWord::BITS;
        self.0[word_idx as usize] ^= 1 << word_shift;
    }
}
