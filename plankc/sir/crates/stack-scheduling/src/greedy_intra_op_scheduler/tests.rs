use super::{GreedyIntraOpScheduler, dedup_unsorted};
use crate::{
    greedy_intra_op_scheduler::greedy_schedule_op,
    op_graph::ValueNodeId,
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};
use allocator_api2::{vec, vec::Vec as AllocVec};
use plank_test_utils::dedent_preserve_blank_lines;
use sir_data::StaticAllocId;
use std::{cell::Cell, collections::HashSet, fmt::Write};

fn assert_dedup_equals<T: PartialEq + std::fmt::Debug, const N: usize>(
    mut start: AllocVec<T>,
    expected: [T; N],
) {
    for i in 0..N {
        for j in i + 1..N {
            assert_ne!(expected[i], expected[j], "expected contains duplicates");
        }
    }
    dedup_unsorted(&mut start);
    assert_eq!(&start, expected.as_slice(), "deduped != expected");
}

#[test]
fn test_dedup_unsorted() {
    assert_dedup_equals::<u32, _>(vec![], []);
    assert_dedup_equals(vec![1, 3, 2], [1, 3, 2]);
    assert_dedup_equals(vec![1, 3, 2, 3], [1, 3, 2]);
    assert_dedup_equals(vec![3, 1, 3, 3, 2], [3, 1, 2]);
    assert_dedup_equals(vec![1, 1, 1, 1], [1]);
}

fn assert_intra_op_schedule_exists(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    start_spilled: impl AsRef<[u32]>,
    target_inputs: impl AsRef<[u32]>,
    last_uses: impl AsRef<[u32]>,
) -> Vec<StackOps> {
    let mut evm_stack = EvmStack::new();
    for &value in start_stack.as_ref().iter().rev() {
        evm_stack.push(ValueNodeId::new(value));
    }

    let mut spilled = Vec::with_capacity(start_spilled.as_ref().len());
    for (alloc, &value) in (0u32..).zip(start_spilled.as_ref()) {
        let value = ValueNodeId::new(value);
        spilled.push((value, StaticAllocId::new(alloc)));
    }

    let target = target_inputs.as_ref().iter().copied().map(ValueNodeId::new).collect::<Vec<_>>();
    let last_uses =
        last_uses.as_ref().iter().copied().map(ValueNodeId::new).collect::<HashSet<_>>();

    let next_alloc_id = Cell::new(StaticAllocId::new(start_spilled.as_ref().len() as u32));
    let mut ops = Vec::new();
    let mut stack =
        TrackedStack::new_from_parts(&next_alloc_id, |op| ops.push(op), evm_stack, spilled);

    for &value in &target {
        assert!(
            stack.fifo().contains(&value) || stack.get_spilled(value).is_some(),
            "target input is neither on the stack nor spilled"
        );
    }

    let mut unique_last_uses = Vec::with_capacity(target.len());
    for &value in &target {
        if last_uses.contains(&value) && !unique_last_uses.contains(&value) {
            unique_last_uses.push(value);
        }
    }

    {
        greedy_schedule_op(arena, config, &mut stack, graph, op_id, complete);

        let end_stack = stack.stack().fifo();
        assert!(end_stack.len() >= target.len(), "end stack is shorter than target inputs");
        assert_eq!(&end_stack[..target.len()], target.as_slice(), "top inputs != target");
    }

    for &op in &ops {
        assert!(op.is_valid(config));
    }

    ops
}

fn assert_intra_op_schedule(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    start_spilled: impl AsRef<[u32]>,
    target_inputs: impl AsRef<[u32]>,
    last_uses: impl AsRef<[u32]>,
    expected_ops: &str,
) {
    let ops = assert_intra_op_schedule_exists(
        config,
        start_stack,
        start_spilled,
        target_inputs,
        last_uses,
    );
    let expected_ops = dedent_preserve_blank_lines(expected_ops);
    pretty_assertions::assert_str_eq!(format_ops(&ops).trim(), expected_ops.trim());
}

fn format_ops(ops: &[StackOps]) -> String {
    let mut out = String::new();
    for &op in ops {
        fmt_stack_op(&mut out, op);
        out.push('\n');
    }
    out
}

fn fmt_stack_op(out: &mut String, op: StackOps) {
    match op {
        StackOps::Swap(depth) => write!(out, "swap {depth}").unwrap(),
        StackOps::Dup(depth) => write!(out, "dup {depth}").unwrap(),
        StackOps::Pop => out.push_str("pop"),
        StackOps::Op(op) => write!(out, "op #{op}").unwrap(),
        StackOps::CallRetPush(op) => write!(out, "call_ret_push #{op}").unwrap(),
        StackOps::Exchange(n, m) => write!(out, "exchange {n} {m}").unwrap(),
        StackOps::Store(alloc) => write!(out, "store :{alloc}").unwrap(),
        StackOps::Load(alloc) => write!(out, "load :{alloc}").unwrap(),
    }
}

#[test]
fn no_inputs() {
    assert_intra_op_schedule(ScheduleConfig::default(), [1, 2], [3], [], [], "");
}

#[test]
fn dup_available_input() {
    assert_intra_op_schedule(
        ScheduleConfig::default(),
        [1],
        [],
        [1],
        [],
        r#"
        dup 0
        "#,
    );
}

#[test]
fn load_spilled_input() {
    assert_intra_op_schedule(
        ScheduleConfig::default(),
        [],
        [7],
        [7],
        [],
        r#"
        load :0
        "#,
    );
}

#[test]
fn last_use_in_place() {
    assert_intra_op_schedule(ScheduleConfig::default(), [7], [], [7], [7], "");
}
