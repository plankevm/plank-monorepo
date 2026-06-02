use super::GreedyShuffler;
use crate::{
    op_graph::ValueNodeId,
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};
use StackOps::{Dup, Pop, Swap};
use sir_data::{Idx, StaticAllocId};
use std::{cell::Cell, collections::HashSet};

fn assert_shuffle(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    target_stack: impl AsRef<[u32]>,
    expected_ops: impl AsRef<[StackOps]>,
) {
    let mut evm_stack = EvmStack::new();
    for &v in start_stack.as_ref().iter().rev() {
        evm_stack.push(ValueNodeId::new(v));
    }

    let target = target_stack.as_ref().iter().map(|v| ValueNodeId::new(*v)).collect::<Vec<_>>();

    let inputs = evm_stack.fifo().iter().copied().collect::<HashSet<_>>();
    let outputs = target.iter().copied().collect::<HashSet<_>>();
    assert!(inputs.is_superset(&outputs), "impossible start/target configuration");

    let next_alloc_id = Cell::new(StaticAllocId::ZERO);
    let mut ops = Vec::new();

    let mut stack = TrackedStack::new_from_evm(&next_alloc_id, |op| ops.push(op), evm_stack, 8);
    GreedyShuffler::run(&mut stack, &target, config);

    assert_eq!(stack.stack().fifo(), target, "end != target");

    for &op in &ops {
        assert!(op.is_valid(config));
    }

    assert_eq!(ops, expected_ops.as_ref());
}

fn store(id: u32) -> StackOps {
    StackOps::Store(StaticAllocId::new(id))
}

fn load(id: u32) -> StackOps {
    StackOps::Load(StaticAllocId::new(id))
}

#[test]
fn noop_smoke() {
    assert_shuffle(ScheduleConfig::default(), [1, 2, 3], [1, 2, 3], []);
}

#[test]
fn pops_unneeded() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [4, 2, 3, 1],
        [1, 2, 3],
        [Pop, Swap(1), Swap(2)],
    );
}

#[test]
fn swaps_top_to_correct_position() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [1, 9, 3, 4],
        [3, 1, 4, 3],
        [Swap(1), Pop, Swap(1), Swap(2), Swap(1), store(0), Dup(1), load(0), Swap(1)],
    );
}

#[test]
fn pops_extra_top_value_single() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [1, 1, 2, 3],
        [1, 2, 3, 2],
        [Pop, Swap(1), Swap(2), Swap(1), store(0), Dup(1), load(0)],
    );
}

#[test]
fn swaps_and_pops_extra_value() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [2, 1, 1, 3],
        [2, 1, 3, 2],
        [Swap(2), Pop, Swap(1), Swap(2), Swap(1), store(0), Dup(1), load(0), Swap(1)],
    );
}

#[test]
fn pops_duplicate_top_value() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [1, 1, 2, 4],
        [1, 1, 4, 2],
        [Pop, Swap(1), Swap(2), Swap(1), Dup(0)],
    );
}

#[test]
fn spills_when_no_shrink_step_applies() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [1, 2, 3, 4],
        [1, 2, 4, 3],
        [store(0), Swap(1), Swap(2), Swap(1), load(0)],
    );
}

#[test]
fn repeatedly_pops_extra_top_values() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [1, 1, 1, 2, 3],
        [1, 2, 3, 2, 3],
        [Pop, Pop, store(0), Dup(1), Dup(1), load(0)],
    );
}

#[test]
fn repeatedly_swaps_and_pops_extra_values() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [2, 1, 1, 3, 3],
        [2, 1, 3, 2, 2],
        [
            Swap(2),
            Pop,
            Swap(2),
            Pop,
            Swap(2),
            store(0),
            Dup(1),
            Swap(1),
            Dup(1),
            Swap(1),
            load(0),
            Swap(2),
        ],
    );
}

#[test]
fn simple_swap_only() {
    assert_shuffle(
        ScheduleConfig::default(),
        [5, 1, 2, 3, 4],
        [1, 2, 3, 4, 5],
        [Swap(4), Swap(3), Swap(2), Swap(1)],
    );
}

#[test]
fn needs_unspill() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(3),
        [1, 2, 3, 4, 5, 6],
        [1, 6, 3, 4, 5, 6],
        [Swap(1), Pop, store(0), store(1), Dup(2), load(1), Swap(1), load(0)],
    );
}
