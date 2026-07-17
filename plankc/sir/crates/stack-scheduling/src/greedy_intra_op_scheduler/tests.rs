use crate::{
    greedy_intra_op_scheduler::greedy_schedule_op,
    op_graph::{OpGraph, OpGraphBuilder, OpNodeId, OpNodeKind, OpSet, ValueNodeId},
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};
use StackOps::*;
use plank_core::Idx;
use proptest::prelude::*;
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

    let complete_backing = vec![0; graph.words_per_set() as usize];
    let complete = OpSet::new(&complete_backing, graph.total_ops());
    let mut ops = Vec::new();

    let mut stack = TrackedStack::new_from_parts(
        StaticAllocId::ZERO,
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

    greedy_schedule_op(config, &mut stack, &graph, op_id, complete);

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
    fn max_swap_depth(mut self, depth: u8) -> Self {
        self.config = ScheduleConfig::max_swap_no_exchange(depth);
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
fn no_inputs_do_nothing() {
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

#[test]
fn simple_swap() {
    opts().last_uses([1, 2]).assert([1, 2], [2, 1], [Swap(1)]);
}

#[test]
fn permutes_below_a_correct_prefix() {
    opts().last_uses([1, 2, 3]).assert([1, 3, 2], [1, 2, 3], [Swap(1), Swap(2), Swap(1)]);
}

#[test]
fn preserves_a_non_last_use_in_the_tail() {
    opts().last_uses([1, 3]).assert([1, 2, 3], [2, 1, 3], [Dup(1), Swap(3), Swap(2)]);
}

#[test]
fn spills_until_a_push_is_in_reach() {
    opts().max_swap_depth(2).assert([9, 8, 1], [1], [store(0), Dup(1)]);
}

#[test]
fn pops_instead_of_spilling_an_already_spilled_value() {
    opts().max_swap_depth(1).spilled([9]).assert([9, 1], [1], [Pop, Dup(0)]);
}

#[test]
fn spills_an_unreachable_swap_top() {
    opts().max_swap_depth(1).last_uses([1]).assert(
        [9, 8, 1],
        [1],
        [store(0), store(1), store(2), load(2)],
    );
}

#[test]
fn spill_and_rebuild_correctly_when_swap_unreachable() {
    opts().max_swap_depth(1).spilled([3]).assert(
        [2, 3],
        [2, 2, 3],
        [load(0), Pop, Dup(0), load(0), Swap(1), Dup(0)],
    );
}

fn unique(values: Vec<u32>) -> Vec<u32> {
    let mut unique = Vec::with_capacity(values.len());
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique
}

fn generated_schedule() -> impl Strategy<Value = (Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>, u8)> {
    (prop::collection::vec(0u32..32, 0..16), prop::collection::vec(0u32..32, 0..16))
        .prop_map(|(stack, spilled)| (unique(stack), unique(spilled)))
        .prop_filter("at least one value must be available", |(stack, spilled)| {
            !stack.is_empty() || !spilled.is_empty()
        })
        .prop_flat_map(|(stack, spilled)| {
            let available = unique(stack.iter().chain(&spilled).copied().collect());
            (
                Just(stack),
                Just(spilled),
                prop::collection::vec(prop::sample::select(available), 0..16),
            )
        })
        .prop_flat_map(|(stack, spilled, target)| {
            let candidates = unique(target.clone());
            let candidate_count = candidates.len();
            (
                Just(stack),
                Just(spilled),
                Just(target),
                prop::sample::subsequence(candidates, 0..=candidate_count),
                1u8..=8,
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn generated_operand_schedules_are_correct(
        (stack, spilled, target, last_uses, max_swap_depth) in generated_schedule(),
    ) {
        let config = ScheduleConfig::max_swap_no_exchange(max_swap_depth);
        assert_intra_op_schedule_exists(config, stack, spilled, target, last_uses);
    }
}
