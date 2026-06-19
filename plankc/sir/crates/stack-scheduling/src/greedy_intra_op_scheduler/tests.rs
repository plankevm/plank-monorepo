use super::dedup_unsorted;
use crate::{
    greedy_intra_op_scheduler::greedy_schedule_op,
    op_graph::{OpGraph, OpGraphBuilder, OpNodeId, OpNodeKind, OpSet, ValueNodeId},
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};
use allocator_api2::{vec, vec::Vec as AllocVec};
use plank_core::Idx;
use plank_test_utils::dedent_preserve_blank_lines;
use sir_data::{OperationIdx, StaticAllocId};
use std::{cell::Cell, collections::HashSet, fmt::Write};
use stumpalo::Arena;

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

fn build_graph(
    start_stack: &[u32],
    start_spilled: &[u32],
    target_inputs: &[u32],
    last_uses: &HashSet<ValueNodeId>,
) -> (OpGraph, OpNodeId) {
    let total_values = start_stack
        .iter()
        .chain(start_spilled)
        .chain(target_inputs)
        .copied()
        .max()
        .map_or(0, |max| max as usize + 1);

    let mut builder = OpGraphBuilder::with_capacity(target_inputs.len() + 1, total_values);
    let mut values = Vec::with_capacity(total_values);
    for _ in 0..total_values {
        values.push(builder.push_input_value());
    }

    let mut builder = builder.end_inputs_begin_ops();
    let op_id = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        for &value in target_inputs {
            op.add_input(values[value as usize]);
        }
        let op_id = op.id();
        drop(op.end_inputs_begin_outputs());
        op_id
    };

    let mut seen_non_last_uses = HashSet::with_capacity(target_inputs.len());
    for &value in target_inputs {
        let value = values[value as usize];
        if !last_uses.contains(&value) && seen_non_last_uses.insert(value) {
            let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            op.add_input(value);
            drop(op.end_inputs_begin_outputs());
        }
    }

    (builder.end_ops_begin_end_stack().finish(), op_id)
}

fn assert_intra_op_schedule_exists(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    start_spilled: impl AsRef<[u32]>,
    target_inputs: impl AsRef<[u32]>,
    last_uses: impl AsRef<[u32]>,
) -> Vec<StackOps> {
    let start_stack = start_stack.as_ref();
    let start_spilled = start_spilled.as_ref();
    let target_inputs = target_inputs.as_ref();
    let last_uses = last_uses.as_ref();

    let last_uses = last_uses.iter().copied().map(ValueNodeId::new).collect::<HashSet<_>>();
    let (graph, op_id) = build_graph(start_stack, start_spilled, target_inputs, &last_uses);
    let target = graph.get_op(op_id).inputs_fifo;

    let mut evm_stack = EvmStack::new();
    for &value in start_stack.iter().rev() {
        evm_stack.push(ValueNodeId::new(value));
    }

    let mut spilled = Vec::with_capacity(start_spilled.len());
    for (alloc, &value) in (0u32..).zip(start_spilled) {
        spilled.push((ValueNodeId::new(value), StaticAllocId::new(alloc)));
    }

    let next_alloc_id = Cell::new(StaticAllocId::new(start_spilled.len() as u32));
    let arena = Arena::new();
    let complete_backing = vec![0; graph.words_per_set() as usize];
    let complete = OpSet::new(&complete_backing, graph.total_ops());
    let mut ops = Vec::new();

    let mut stack =
        TrackedStack::new_from_parts(&next_alloc_id, |op| ops.push(op), evm_stack, spilled);

    for &value in target {
        assert!(
            stack.fifo().contains(&value) || stack.get_spilled(value).is_some(),
            "target input is neither on the stack nor spilled"
        );
    }

    greedy_schedule_op(arena.as_arena_ref(), config, &mut stack, &graph, op_id, complete);

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
    assert_intra_op_schedule(
        ScheduleConfig::default(),
        [1, 2],
        [3],
        [],
        [],
        r#"
        op #0
        "#,
    );
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
        op #0
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
        op #0
        "#,
    );
}

#[test]
fn last_use_in_place() {
    assert_intra_op_schedule(
        ScheduleConfig::default(),
        [7],
        [],
        [7],
        [7],
        r#"
        op #0
        "#,
    );
}
