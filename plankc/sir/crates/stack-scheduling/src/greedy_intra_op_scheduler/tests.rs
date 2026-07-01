use crate::{
    greedy_intra_op_scheduler::greedy_schedule_op,
    op_graph::{OpGraph, OpGraphBuilder, OpNodeId, OpNodeKind, OpSet, ValueNodeId},
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};
use StackOps::*;
use plank_core::Idx;
use sir_data::{OperationIdx, StaticAllocId};
use std::collections::HashSet;

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

    let next_alloc_id = StaticAllocId::new(start_spilled.len() as u32);
    let complete_backing = vec![0; graph.words_per_set() as usize];
    let complete = OpSet::new(&complete_backing, graph.total_ops());
    let mut ops = Vec::new();

    let mut stack = TrackedStack::new_from_parts(
        next_alloc_id,
        |op| ops.push(op),
        evm_stack.fifo(),
        start_spilled.iter().map(|&v| ValueNodeId::new(v)).collect(),
    );

    for &value in target {
        assert!(
            stack.fifo().contains(&value) || stack.get_spilled(value).is_some(),
            "target input is neither on the stack nor spilled"
        );
    }

    greedy_schedule_op(config, &mut stack, &graph, op_id, complete, 4);

    for &op in &ops {
        assert!(op.is_valid(config));
    }

    let last = ops.pop();
    assert_eq!(last, Some(Op(OperationIdx::new(0))), "last op not `Op(...)`");

    ops
}

fn assert_intra_op_schedule(
    config: ScheduleConfig,
    start_stack: impl AsRef<[u32]>,
    start_spilled: impl AsRef<[u32]>,
    target_inputs: impl AsRef<[u32]>,
    last_uses: impl AsRef<[u32]>,
    expected_ops: impl AsRef<[StackOps]>,
) {
    let ops = assert_intra_op_schedule_exists(
        config,
        start_stack,
        start_spilled,
        target_inputs,
        last_uses,
    );
    let expected = expected_ops.as_ref();
    assert_eq!(&ops, expected);
}

const fn store(id: u32) -> StackOps {
    StackOps::Store(StaticAllocId::new(id))
}

const fn load(id: u32) -> StackOps {
    StackOps::Load(StaticAllocId::new(id))
}

struct AssertScheduleBuilder {
    config: ScheduleConfig,
    spilled: Vec<u32>,
    last_uses: Vec<u32>,
}

impl AssertScheduleBuilder {
    fn config(mut self, c: ScheduleConfig) -> Self {
        self.config = c;
        self
    }

    fn spilled(mut self, values: impl AsRef<[u32]>) -> Self {
        self.spilled = values.as_ref().into();
        self
    }

    fn last_uses(mut self, values: impl AsRef<[u32]>) -> Self {
        self.last_uses = values.as_ref().into();
        self
    }

    fn assert(
        self,
        start: impl AsRef<[u32]>,
        target: impl AsRef<[u32]>,
        expected: impl AsRef<[StackOps]>,
    ) {
        assert_intra_op_schedule(
            self.config,
            start,
            self.spilled,
            target,
            self.last_uses,
            expected,
        );
    }
}

fn opts() -> AssertScheduleBuilder {
    AssertScheduleBuilder {
        config: ScheduleConfig::default(),
        spilled: Vec::new(),
        last_uses: Vec::new(),
    }
}

#[test]
fn no_inputs() {
    opts().spilled([3]).assert([1, 2], [], []);
}

#[test]
fn dup_available_input() {
    opts().assert([1], [1], [Dup(0)]);
}

#[test]
fn load_spilled_input() {
    opts().spilled([7]).assert([], [7], [load(0)]);
}

#[test]
fn last_use_in_place() {
    opts().last_uses([7]).assert([7], [7], []);
}
