// TODO: Actually use in new scheduler
#![allow(unused)]

use crate::op_graph::{BitsetWord, OpGraph, OpSet, OpSetMut, ValueNodeId};
use plank_core::{Idx, IndexVec, newtype_index};
use sir_data::StaticAllocId;
use std::num::NonZero;

newtype_index! {
    struct ArenaIdx;
}

#[derive(Debug, Clone, Copy)]
pub struct StateHandle {
    cumulative_gas_cost: u32,
    complete_bitset_idx: u32,

    ids_start: ArenaIdx,
    total_spilled: u32,
    stack_depth: u16,
}

#[derive(Debug)]
pub(crate) struct ScheduleStateArena<'g> {
    complete_bitsets_arena: Vec<BitsetWord>,
    /// Holds `[(spilled_values*, spilled_allocs*, stack_value*)]`
    ids_arena: IndexVec<ArenaIdx, NonZero<u32>>,
    graph: &'g OpGraph,
}

pub(crate) struct StateView<'a> {
    pub cumulative_gas_cost: u32,
    pub complete: OpSet<'a>,
    pub stack: &'a [ValueNodeId],
    pub spilled_values: &'a [ValueNodeId],
    pub spilled_allocs: &'a [StaticAllocId],
    pub graph: &'a OpGraph,
}

impl<'g> ScheduleStateArena<'g> {
    pub fn graph(&self) -> &'g OpGraph {
        self.graph
    }

    pub fn new(graph: &'g OpGraph) -> Self {
        Self { complete_bitsets_arena: Vec::new(), ids_arena: IndexVec::new(), graph }
    }

    pub fn total_bitsets(&self) -> usize {
        self.complete_bitsets_arena
            .len()
            .checked_div(self.graph.words_per_set() as usize)
            .unwrap_or(0)
    }

    pub fn alloc_initial(&mut self) -> StateHandle {
        let complete_bitset_idx = self.total_bitsets().try_into().expect("overflow");
        self.complete_bitsets_arena
            .extend(std::iter::repeat_n(0, self.graph.words_per_set() as usize));

        let ids_start = self.ids_arena.len_idx();
        let input_values = self.graph.input_values_fifo();
        self.ids_arena.extend(input_values.iter().map(|value| value.to_raw()));

        StateHandle {
            cumulative_gas_cost: 0,
            complete_bitset_idx,
            ids_start,
            total_spilled: 0,
            stack_depth: input_values.len().try_into().expect("overflow"),
        }
    }

    pub fn reset(&mut self, graph: &'g OpGraph) {
        self.complete_bitsets_arena.clear();
        self.ids_arena.clear();
        self.graph = graph;
    }

    pub fn get_state(&self, state: StateHandle) -> StateView<'_> {
        let set_size = self.graph.words_per_set() as usize;
        let complete_start = state.complete_bitset_idx as usize * set_size;
        let complete = OpSet::new(
            &self.complete_bitsets_arena[complete_start..][..set_size],
            self.graph.total_ops(),
        );

        let mut arena_offset = state.ids_start;
        let spilled_values = ValueNodeId::cast_from_slice({
            let raw_slice = &self.ids_arena[arena_offset..][..state.total_spilled as usize];
            arena_offset += state.total_spilled;
            raw_slice
        });
        let spilled_allocs = StaticAllocId::cast_from_slice({
            let raw_slice = &self.ids_arena[arena_offset..][..state.total_spilled as usize];
            arena_offset += state.total_spilled;
            raw_slice
        });
        let stack = ValueNodeId::cast_from_slice({
            let raw_slice = &self.ids_arena[arena_offset..][..state.stack_depth as usize];
            arena_offset += state.stack_depth.into();
            raw_slice
        });

        StateView {
            cumulative_gas_cost: state.cumulative_gas_cost,
            complete,
            stack,
            spilled_values,
            spilled_allocs,
            graph: self.graph,
        }
    }
}

impl StateView<'_> {
    pub fn is_last_use(&self, value: ValueNodeId) -> bool {
        self.graph.uses_remaining(&self.complete, value) == 1
    }
}

pub fn collect_next_completable_into(graph: &OpGraph, complete: OpSet<'_>, out: &mut OpSetMut<'_>) {
    for op in graph.op_ids() {
        if !complete.contains(op) && complete.is_super(&graph.get_predecessors(op)) {
            out.add(op);
        }
    }
}
