use super::GreedyShuffler;
use crate::{
    op_graph::ValueNodeId,
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};
use StackOps::{Dup, Pop, Swap};
use proptest::prelude::*;
use sir_data::{Idx, StaticAllocId};
use std::{cell::Cell, collections::HashSet};

fn assert_shuffle_exists(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    target_stack: impl AsRef<[u32]>,
) -> Vec<StackOps> {
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
    GreedyShuffler::run(config, &mut stack, &target);

    assert_eq!(stack.stack().fifo(), target, "end != target");

    for &op in &ops {
        assert!(op.is_valid(config));
    }

    ops
}

fn assert_shuffle(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    target_stack: impl AsRef<[u32]>,
    expected_ops: impl AsRef<[StackOps]>,
) {
    let ops = assert_shuffle_exists(config, start_stack, target_stack);
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

#[test]
fn current_is_already_correct_prefix() {
    assert_shuffle(ScheduleConfig::max_swap_no_exchange(2), [1, 0], [0], [Pop]);
}

#[test]
fn correct_after_swap_but_trash_top() {
    assert_shuffle(ScheduleConfig::default(), [1, 3, 2], [1, 2], [Swap(1), Pop]);
}

#[test]
fn empty_to_empty() {
    assert_shuffle(ScheduleConfig::default(), [], [], []);
}

#[test]
fn pop_once() {
    assert_shuffle(ScheduleConfig::max_swap_no_exchange(1), [1], [], [Pop]);
}

#[test]
fn pop_thrice() {
    assert_shuffle(ScheduleConfig::max_swap_no_exchange(1), [0, 0, 0], [], [Pop, Pop, Pop]);
}

#[test]
fn pop_lower2() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(1),
        [0, 1, 2],
        [0],
        [Swap(1), Pop, Swap(1), Pop],
    );
}

#[test]
fn unspill_horizon_before_dup_top1() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(1),
        [0, 1],
        [1, 1, 0, 1],
        [store(0), Dup(0), load(0), Swap(1), Dup(0)],
    );
}

#[test]
fn unspill_horizon_before_dup_top2() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(1),
        [0, 1],
        [0, 0, 1, 1, 0],
        [Swap(1), store(0), Dup(0), load(0), Swap(1), load(0), Swap(1), Dup(0)],
    );
}

#[test]
fn intricate_spill_dup_swap() {
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(3),
        [10, 17, 2],
        [10, 2, 2, 10, 17, 17],
        [Dup(1), Swap(3), Dup(1), Dup(1), Swap(1)],
    );
}

fn shuffle_case() -> impl Strategy<Value = (ScheduleConfig, Vec<u32>, Vec<u32>)> {
    (1u8..=6, prop::collection::vec(0u32..30, 1..=20)).prop_flat_map(|(max_swap, start)| {
        let values = start.clone();
        let target = prop::collection::vec(prop::sample::select(values), 0..=20);

        (Just(ScheduleConfig::max_swap_no_exchange(max_swap)), Just(start), target)
    })
}

proptest! {
    #[test]
    fn successfully_shuffles((config, start, target) in shuffle_case()) {
        assert_shuffle_exists(config, start, target);
    }
}
