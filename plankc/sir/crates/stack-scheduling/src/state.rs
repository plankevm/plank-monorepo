use crate::op_graph::{BitsetWord, OpGraph, OpSet, ValueNodeId};
use plank_core::{IndexVec, newtype_index};
use sir_data::StaticAllocId;

newtype_index! {
    struct ValueArenaIdx;
    struct SpillAllocIdArenaIdx;
}

#[derive(Debug, Clone, Copy)]
pub struct StoredScheduleState {
    cumulative_gas_cost: u32,
    complete_bitset_idx: u32,

    values_start: ValueArenaIdx,
    stack_depth: u16,
    total_spilled: u32,
    spill_alloc_start: SpillAllocIdArenaIdx,
}

#[derive(Debug)]
struct ScheduleStateArena {
    complete_bitsets_arena: Vec<BitsetWord>,
    /// Holds `[(spilled_value*, stack_value*)]`
    values_arena: IndexVec<ValueArenaIdx, ValueNodeId>,
    spilled_arena: IndexVec<SpillAllocIdArenaIdx, StaticAllocId>,
}

impl ScheduleStateArena {
    fn clear(&mut self) {
        self.complete_bitsets_arena.clear();
        self.values_arena.clear();
        self.spilled_arena.clear();
    }
}

struct ScheduledState<'a> {
    cumulative_gas_cost: u32,
    complete: OpSet<'a>,
    stack: &'a [ValueNodeId],
    spilled_values: &'a [ValueNodeId],
    spilled_allocs: &'a [StaticAllocId],
}

impl ScheduleStateArena {
    fn get_state(&self, graph: &OpGraph, state: StoredScheduleState) -> ScheduledState<'_> {
        let words_per_complete = graph.total_ops().div_ceil(BitsetWord::BITS);
        let complete_start = state.complete_bitset_idx * words_per_complete;
        let complete = (&self.complete_bitsets_arena[complete_start as usize..]
            [..words_per_complete as usize])
            .into();

        let stack_start = state.values_start + state.total_spilled;
        let stack = &self.values_arena[stack_start..][..state.stack_depth as usize];

        let total_spilled = state.total_spilled as usize;
        let spilled_values = &self.values_arena[state.values_start..][..total_spilled];
        let spilled_allocs = &self.spilled_arena[state.spill_alloc_start..][..total_spilled];

        ScheduledState {
            cumulative_gas_cost: state.cumulative_gas_cost,
            complete,
            stack,
            spilled_values,
            spilled_allocs,
        }
    }
}
