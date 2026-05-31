use super::GreedyShuffler;
use crate::{
    op_graph::ValueNodeId,
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};
use sir_data::{Idx, StaticAllocId};
use std::cell::Cell;

fn assert_shuffle(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    target_stack: impl AsRef<[u32]>,
    expected_out: impl AsRef<[u32]>,
    expected_ops: impl AsRef<[StackOps]>,
) {
    let mut evm_stack = EvmStack::new();
    for &v in start_stack.as_ref().iter().rev() {
        evm_stack.push(ValueNodeId::new(v));
    }

    let target = target_stack.as_ref().iter().map(|v| ValueNodeId::new(*v)).collect::<Vec<_>>();

    let next_alloc_id = Cell::new(StaticAllocId::ZERO);
    let mut ops = Vec::new();

    let mut stack = TrackedStack::new_from_evm(&next_alloc_id, |op| ops.push(op), evm_stack, 8);
    GreedyShuffler::run(&mut stack, &target, config);

    let expected_out =
        expected_out.as_ref().iter().map(|v| ValueNodeId::new(*v)).collect::<Vec<_>>();
    assert_eq!(stack.stack().fifo(), expected_out);

    for &op in &ops {
        assert!(op.is_valid(config));
    }

    assert_eq!(ops, expected_ops.as_ref());
}

fn assert_shuffle_complete(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    target_stack: impl AsRef<[u32]>,
    expected_ops: impl AsRef<[StackOps]>,
) {
    let target_stack = target_stack.as_ref();
    assert_shuffle(config, start_stack, target_stack, target_stack, expected_ops);
}

#[test]
fn no_op_smoke_test() {
    assert_shuffle_complete(ScheduleConfig::default(), [1, 2, 3], [1, 2, 3], []);
}

#[test]
fn pops_unneeded() {
    use StackOps::*;
    assert_shuffle(
        ScheduleConfig::max_swap_no_exchange(2),
        [4, 2, 3, 1],
        [1, 2, 3],
        [2, 3, 1],
        [Pop],
    );
}
