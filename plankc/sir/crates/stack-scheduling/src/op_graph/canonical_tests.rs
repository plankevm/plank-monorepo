use std::fmt::Write;

use plank_core::{Idx, IndexVec};
use plank_test_utils::dedent_preserve_blank_lines;
use pretty_assertions::assert_str_eq;
use sir_data::OperationIdx;

use super::*;
use crate::op_graph::{OpGraphBuilder, OpNodeKind};

fn format_graph(out: &mut String, heading: &str, graph: &OpGraph, finalization: BlockFinalization) {
    writeln!(out, "{heading}:").unwrap();
    write_values(out, "  inputs", graph.input_values_fifo().iter());
    for operation in graph.op_ids() {
        let op = graph.get_op(operation);
        writeln!(out, "  op{} {}", operation.get(), kind_name(op.kind)).unwrap();
        write_values(out, "    inputs", op.inputs_fifo.iter().copied());
        write_values(out, "    outputs", op.outputs_fifo.iter().copied());
        write_operations(out, "    predecessors", graph.displayed_predecessors(operation));
    }
    write_values(out, "  outputs", graph.output_values_fifo().iter().copied());
    writeln!(
        out,
        "  finalization: {}",
        match finalization {
            BlockFinalization::ShuffleToOutputs => "shuffle-to-outputs",
            BlockFinalization::LastOpTerminates => "last-op-terminates",
        }
    )
    .unwrap();
}

fn kind_name(kind: OpNodeKind) -> &'static str {
    match kind {
        OpNodeKind::Normal(_) => "normal",
        OpNodeKind::Flippable(_) => "flippable",
        OpNodeKind::RetDestPush(_) => "ret-dest-push",
    }
}

fn write_values(out: &mut String, label: &str, values: impl IntoIterator<Item = ValueNodeId>) {
    write!(out, "{label}: [").unwrap();
    for (position, value) in values.into_iter().enumerate() {
        if position != 0 {
            out.push_str(", ");
        }
        write!(out, "v{}", value.get()).unwrap();
    }
    out.push_str("]\n");
}

fn write_operations(out: &mut String, label: &str, operations: impl IntoIterator<Item = OpNodeId>) {
    write!(out, "{label}: [").unwrap();
    for (position, operation) in operations.into_iter().enumerate() {
        if position != 0 {
            out.push_str(", ");
        }
        write!(out, "op{}", operation.get()).unwrap();
    }
    out.push_str("]\n");
}

fn key(graph: &OpGraph, finalization: BlockFinalization) -> CanonicalBlockKey {
    canonicalize_graph(graph, finalization).deduplication_key()
}

#[test]
fn canonical_block_json_round_trip_is_stable() {
    let block = CanonicalBlock::new(
        BlockFinalization::ShuffleToOutputs,
        2,
        Box::new([CanonicalOperation {
            inputs_fifo: Box::new([CanonicalValueId::ZERO, CanonicalValueId::ZERO + 1]),
            output_count: 1,
            effect_predecessors: Box::new([]),
            flippable: true,
        }]),
        Box::new([CanonicalValueId::ZERO + 2]),
    );
    let encoded = serde_json::to_string(&block).unwrap();
    assert_eq!(
        encoded,
        r#"{"finalization":"shuffle_to_outputs","input_count":2,"operations":[{"inputs_fifo":[0,1],"output_count":1,"effect_predecessors":[],"flippable":true}],"outputs_fifo":[2]}"#
    );
    assert_eq!(serde_json::from_str::<CanonicalBlock>(&encoded).unwrap(), block);
}

fn format_canonical_graph(out: &mut String, heading: &str, canonicalized: &CanonicalizedBlock) {
    writeln!(out, "{heading}:").unwrap();
    let graph = canonicalized.block().to_op_graph().unwrap();
    for line in crate::display::graph(&graph).lines() {
        writeln!(out, "  {line}").unwrap();
    }
}

fn format_canonical_metadata(out: &mut String, heading: &str, canonicalized: &CanonicalizedBlock) {
    writeln!(out, "{heading}:").unwrap();
    writeln!(
        out,
        "  finalization: {}",
        if canonicalized.last_op_terminates() {
            "last-op-terminates"
        } else {
            "shuffle-to-outputs"
        }
    )
    .unwrap();
    writeln!(out, "  witness:").unwrap();
    for operation in canonicalized.canonical_op_ids() {
        writeln!(
            out,
            "    op{}: source op{}, first-two-inputs-swapped: {}",
            operation.get(),
            canonicalized.source_op(operation).get(),
            canonicalized.first_two_inputs_swapped(operation)
        )
        .unwrap();
    }
}

fn format_case(
    out: &mut String,
    heading: &str,
    graph: &OpGraph,
    finalization: BlockFinalization,
) -> CanonicalBlockKey {
    format_graph(out, &format!("{heading} source"), graph, finalization);
    let canonicalized = canonicalize_graph(graph, finalization);
    format_canonical_graph(out, &format!("{heading} canonical graph"), &canonicalized);
    format_canonical_metadata(out, &format!("{heading} canonical metadata"), &canonicalized);
    canonicalized.deduplication_key()
}

fn assert_graphs(
    left: &(OpGraph, BlockFinalization),
    right: &(OpGraph, BlockFinalization),
    expected: &str,
) -> (CanonicalBlockKey, CanonicalBlockKey) {
    let mut actual = String::new();
    let left_key = format_case(&mut actual, "A", &left.0, left.1);
    actual.push('\n');
    let right_key = format_case(&mut actual, "B", &right.0, right.1);
    let expected = dedent_preserve_blank_lines(expected);
    assert_str_eq!(actual.trim(), expected.trim());
    (left_key, right_key)
}

fn assert_canonicalizes_equal(
    left: &(OpGraph, BlockFinalization),
    right: &(OpGraph, BlockFinalization),
    expected: &str,
) {
    let (left_key, right_key) = assert_graphs(left, right, expected);
    assert_eq!(left_key, right_key);
}

fn assert_canonicalizes_different(
    left: &(OpGraph, BlockFinalization),
    right: &(OpGraph, BlockFinalization),
    expected: &str,
) {
    let (left_key, right_key) = assert_graphs(left, right, expected);
    assert_ne!(left_key, right_key);
}

fn assert_canonicalizes_to(
    graph: &(OpGraph, BlockFinalization),
    expected: &str,
) -> CanonicalBlockKey {
    let mut actual = String::new();
    let key = format_case(&mut actual, "Graph", &graph.0, graph.1);
    let expected = dedent_preserve_blank_lines(expected);
    assert_str_eq!(actual.trim(), expected.trim());
    key
}

fn push_unary(
    graph: &mut OpGraphBuilder<crate::op_graph::builder::AddingGraphOps>,
    kind: OpNodeKind,
    input: ValueNodeId,
) -> ValueNodeId {
    let mut operation = graph.begin_op(kind);
    operation.add_input(input);
    operation.end_inputs_begin_outputs().add_output()
}

fn reordered_graph(reverse: bool) -> (OpGraph, BlockFinalization) {
    let mut source_operations = IndexVec::<OperationIdx, ()>::new();
    let mut graph = OpGraphBuilder::with_capacity(3, 5);
    let left_input = graph.push_input_value();
    let right_input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();

    let (left, right) = if reverse {
        let right =
            push_unary(&mut graph, OpNodeKind::Normal(source_operations.push(())), right_input);
        let left =
            push_unary(&mut graph, OpNodeKind::Normal(source_operations.push(())), left_input);
        (left, right)
    } else {
        let left =
            push_unary(&mut graph, OpNodeKind::Normal(source_operations.push(())), left_input);
        let right =
            push_unary(&mut graph, OpNodeKind::Normal(source_operations.push(())), right_input);
        (left, right)
    };

    let mut combine = graph.begin_op(OpNodeKind::Normal(source_operations.push(())));
    combine.add_input(left);
    combine.add_input(right);
    let output = combine.end_inputs_begin_outputs().add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(output);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn tied_source_graph(reverse: bool) -> (OpGraph, BlockFinalization) {
    permuted_tied_graph(reverse, false)
}

fn permuted_tied_graph(
    reverse_sources: bool,
    reverse_consumers: bool,
) -> (OpGraph, BlockFinalization) {
    let mut source_operations = IndexVec::<OperationIdx, ()>::new();
    let mut graph = OpGraphBuilder::with_capacity(4, 5);
    let input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();

    let (single_use_source, repeated_use_source) = if reverse_sources {
        let repeated =
            push_unary(&mut graph, OpNodeKind::Normal(source_operations.push(())), input);
        let single = push_unary(&mut graph, OpNodeKind::Normal(source_operations.push(())), input);
        (single, repeated)
    } else {
        let single = push_unary(&mut graph, OpNodeKind::Normal(source_operations.push(())), input);
        let repeated =
            push_unary(&mut graph, OpNodeKind::Normal(source_operations.push(())), input);
        (single, repeated)
    };

    let (single_output, repeated_output) = if reverse_consumers {
        let mut repeated_consumer = graph.begin_op(OpNodeKind::Normal(source_operations.push(())));
        repeated_consumer.add_input(repeated_use_source);
        repeated_consumer.add_input(repeated_use_source);
        let repeated = repeated_consumer.end_inputs_begin_outputs().add_output();
        let single = push_unary(
            &mut graph,
            OpNodeKind::Normal(source_operations.push(())),
            single_use_source,
        );
        (single, repeated)
    } else {
        let single = push_unary(
            &mut graph,
            OpNodeKind::Normal(source_operations.push(())),
            single_use_source,
        );
        let mut repeated_consumer = graph.begin_op(OpNodeKind::Normal(source_operations.push(())));
        repeated_consumer.add_input(repeated_use_source);
        repeated_consumer.add_input(repeated_use_source);
        let repeated = repeated_consumer.end_inputs_begin_outputs().add_output();
        (single, repeated)
    };

    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(single_output);
    graph.push_end_stack_value(repeated_output);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn zero_to_one_graph(kind: fn(OperationIdx) -> OpNodeKind) -> (OpGraph, BlockFinalization) {
    let mut source_operations = IndexVec::<OperationIdx, ()>::new();
    let graph = OpGraphBuilder::with_capacity(1, 1);
    let mut graph = graph.end_inputs_begin_ops();
    let output =
        graph.begin_op(kind(source_operations.push(()))).end_inputs_begin_outputs().add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(output);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn binary_graph(flippable: bool, reverse_inputs: bool) -> (OpGraph, BlockFinalization) {
    let mut source_operations = IndexVec::<OperationIdx, ()>::new();
    let mut graph = OpGraphBuilder::with_capacity(1, 3);
    let left = graph.push_input_value();
    let right = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let source_operation = source_operations.push(());
    let kind = if flippable {
        OpNodeKind::Flippable(source_operation)
    } else {
        OpNodeKind::Normal(source_operation)
    };
    let mut operation = graph.begin_op(kind);
    if reverse_inputs {
        operation.add_input(right);
        operation.add_input(left);
    } else {
        operation.add_input(left);
        operation.add_input(right);
    }
    let output = operation.end_inputs_begin_outputs().add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(output);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn ordering_graph(ordered: bool) -> (OpGraph, BlockFinalization) {
    let mut source_operations = IndexVec::<OperationIdx, ()>::new();
    let graph = OpGraphBuilder::with_capacity(2, 0);
    let mut graph = graph.end_inputs_begin_ops();
    let first = graph.begin_op(OpNodeKind::Normal(source_operations.push(()))).id();
    let mut second = graph.begin_op(OpNodeKind::Normal(source_operations.push(())));
    if ordered {
        second.add_predecessor(first);
    }
    (graph.end_ops_begin_end_stack().finish(), BlockFinalization::ShuffleToOutputs)
}

fn empty_graph(finalization: BlockFinalization) -> (OpGraph, BlockFinalization) {
    empty_graph_with_inputs(0, finalization)
}

fn empty_graph_with_inputs(
    input_count: usize,
    finalization: BlockFinalization,
) -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(0, input_count);
    for _ in 0..input_count {
        graph.push_input_value();
    }
    let graph = graph.end_inputs_begin_ops().end_ops_begin_end_stack().finish();
    (graph, finalization)
}

fn output_count_graph(output_count: usize) -> (OpGraph, BlockFinalization) {
    let graph = OpGraphBuilder::with_capacity(1, output_count);
    let mut graph = graph.end_inputs_begin_ops();
    let operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    let mut operation = operation.end_inputs_begin_outputs();
    for _ in 0..output_count {
        operation.add_output();
    }
    (graph.end_ops_begin_end_stack().finish(), BlockFinalization::ShuffleToOutputs)
}

fn input_count_graph(repeated: bool) -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(1, 2);
    let input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    operation.add_input(input);
    if repeated {
        operation.add_input(input);
    }
    let output = operation.end_inputs_begin_outputs().add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(output);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn output_order_graph(reverse: bool) -> (OpGraph, BlockFinalization) {
    let graph = OpGraphBuilder::with_capacity(1, 2);
    let mut graph = graph.end_inputs_begin_ops();
    let operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    let mut operation = operation.end_inputs_begin_outputs();
    let first = operation.add_output();
    let second = operation.add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    if reverse {
        graph.push_end_stack_value(second);
        graph.push_end_stack_value(first);
    } else {
        graph.push_end_stack_value(first);
        graph.push_end_stack_value(second);
    }
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn output_multiplicity_graph(repeated: bool) -> (OpGraph, BlockFinalization) {
    let graph = OpGraphBuilder::with_capacity(1, 1);
    let mut graph = graph.end_inputs_begin_ops();
    let output = graph
        .begin_op(OpNodeKind::Normal(OperationIdx::ZERO))
        .end_inputs_begin_outputs()
        .add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(output);
    if repeated {
        graph.push_end_stack_value(output);
    }
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn transitive_ordering_graph(redundant_edge: bool) -> (OpGraph, BlockFinalization) {
    let graph = OpGraphBuilder::with_capacity(3, 0);
    let mut graph = graph.end_inputs_begin_ops();
    let first = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO)).id();
    let second = {
        let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        operation.add_predecessor(first);
        operation.id()
    };
    let mut third = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    if redundant_edge {
        third.add_predecessor(first);
    }
    third.add_predecessor(second);
    (graph.end_ops_begin_end_stack().finish(), BlockFinalization::ShuffleToOutputs)
}

fn data_dependency_graph() -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(2, 2);
    let input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let produced = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
    let _ = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), produced);
    (graph.end_ops_begin_end_stack().finish(), BlockFinalization::ShuffleToOutputs)
}

fn push_two_output_unary(
    graph: &mut OpGraphBuilder<crate::op_graph::builder::AddingGraphOps>,
    input: ValueNodeId,
) -> [ValueNodeId; 2] {
    let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    operation.add_input(input);
    let mut operation = operation.end_inputs_begin_outputs();
    [operation.add_output(), operation.add_output()]
}

fn multi_output_graph(reverse: bool, exchange_roles: bool) -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(3, 6);
    let input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let (left, right) = if reverse {
        let right = push_two_output_unary(&mut graph, input);
        let left = push_two_output_unary(&mut graph, input);
        (left, right)
    } else {
        let left = push_two_output_unary(&mut graph, input);
        let right = push_two_output_unary(&mut graph, input);
        (left, right)
    };
    let mut consumer = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    if exchange_roles {
        consumer.add_input(left[1]);
        consumer.add_input(right[0]);
    } else {
        consumer.add_input(left[0]);
        consumer.add_input(right[1]);
    }
    let result = consumer.end_inputs_begin_outputs().add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(left[1]);
    graph.push_end_stack_value(result);
    graph.push_end_stack_value(right[0]);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn tied_output_position_graph(reverse: bool) -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(2, 3);
    let input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let (left, right) = if reverse {
        let right = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
        let left = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
        (left, right)
    } else {
        let left = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
        let right = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
        (left, right)
    };
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(left);
    graph.push_end_stack_value(right);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn tied_operand_position_graph(reverse: bool) -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(3, 4);
    let input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let (left, right) = if reverse {
        let right = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
        let left = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
        (left, right)
    } else {
        let left = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
        let right = push_unary(&mut graph, OpNodeKind::Normal(OperationIdx::ZERO), input);
        (left, right)
    };
    let mut consumer = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    consumer.add_input(left);
    consumer.add_input(right);
    let result = consumer.end_inputs_begin_outputs().add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(result);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn effect_tie_graph(reverse: bool) -> (OpGraph, BlockFinalization) {
    let graph = OpGraphBuilder::with_capacity(4, 1);
    let mut graph = graph.end_inputs_begin_ops();
    let (output_source, empty_source) = if reverse {
        let empty = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO)).id();
        let output = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO)).id();
        (output, empty)
    } else {
        let output = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO)).id();
        let empty = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO)).id();
        (output, empty)
    };
    let output = {
        let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        operation.add_predecessor(output_source);
        operation.end_inputs_begin_outputs().add_output()
    };
    let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    operation.add_predecessor(empty_source);
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(output);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn ternary_flippable_graph(order: [usize; 3]) -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(1, 4);
    let inputs = [graph.push_input_value(), graph.push_input_value(), graph.push_input_value()];
    let mut graph = graph.end_inputs_begin_ops();
    let mut operation = graph.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
    for position in order {
        operation.add_input(inputs[position]);
    }
    let output = operation.end_inputs_begin_outputs().add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(output);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn repeated_flippable_input_graph() -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(1, 2);
    let input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let mut operation = graph.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
    operation.add_input(input);
    operation.add_input(input);
    let output = operation.end_inputs_begin_outputs().add_output();
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(output);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

fn kitchen_sink_graph() -> (OpGraph, BlockFinalization) {
    let mut graph = OpGraphBuilder::with_capacity(3, 5);
    let left = graph.push_input_value();
    let right = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let mut producer = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    producer.add_input(left);
    let mut producer = producer.end_inputs_begin_outputs();
    let first = producer.add_output();
    let second = producer.add_output();
    let mut flippable = graph.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
    flippable.add_input(right);
    flippable.add_input(first);
    let flippable_id = flippable.id();
    let result = flippable.end_inputs_begin_outputs().add_output();
    let mut effectful = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
    effectful.add_input(second);
    effectful.add_predecessor(flippable_id);
    let mut graph = graph.end_ops_begin_end_stack();
    graph.push_end_stack_value(result);
    graph.push_end_stack_value(second);
    graph.push_end_stack_value(result);
    (graph.finish(), BlockFinalization::ShuffleToOutputs)
}

#[test]
fn equal_when_independent_operations_are_reordered() {
    assert_canonicalizes_equal(
        &reordered_graph(false),
        &reordered_graph(true),
        r#"
            A source:
              inputs: [v0, v1]
              op0 normal
                inputs: [v0]
                outputs: [v2]
                predecessors: []
              op1 normal
                inputs: [v1]
                outputs: [v3]
                predecessors: []
              op2 normal
                inputs: [v2, v3]
                outputs: [v4]
                predecessors: [op0, op1]
              outputs: [v4]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0, v1]
              v2 = op0(v0)
              v3 = op1(v1)
              v4 = op2(v2, v3)
              outputs: [v4]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false

            B source:
              inputs: [v0, v1]
              op0 normal
                inputs: [v1]
                outputs: [v2]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v3]
                predecessors: []
              op2 normal
                inputs: [v3, v2]
                outputs: [v4]
                predecessors: [op0, op1]
              outputs: [v4]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0, v1]
              v2 = op0(v0)
              v3 = op1(v1)
              v4 = op2(v2, v3)
              outputs: [v4]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op1, first-two-inputs-swapped: false
                op1: source op0, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn equal_when_tied_operations_with_different_consumers_are_reordered() {
    assert_canonicalizes_equal(
        &tied_source_graph(false),
        &tied_source_graph(true),
        r#"
            A source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v2]
                predecessors: []
              op2 normal
                inputs: [v1]
                outputs: [v3]
                predecessors: [op0]
              op3 normal
                inputs: [v2, v2]
                outputs: [v4]
                predecessors: [op1]
              outputs: [v3, v4]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0]
              v1 = op0(v0)
              v2 = op1(v0)
              v3 = op2(v1, v1)
              v4 = op3(v2)
              outputs: [v4, v3]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op1, first-two-inputs-swapped: false
                op1: source op0, first-two-inputs-swapped: false
                op2: source op3, first-two-inputs-swapped: false
                op3: source op2, first-two-inputs-swapped: false

            B source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v2]
                predecessors: []
              op2 normal
                inputs: [v2]
                outputs: [v3]
                predecessors: [op1]
              op3 normal
                inputs: [v1, v1]
                outputs: [v4]
                predecessors: [op0]
              outputs: [v3, v4]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0]
              v1 = op0(v0)
              v2 = op1(v0)
              v3 = op2(v1, v1)
              v4 = op3(v2)
              outputs: [v4, v3]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
                op2: source op3, first-two-inputs-swapped: false
                op3: source op2, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn equal_for_normal_and_return_destination_push_with_the_same_arity() {
    assert_canonicalizes_equal(
        &zero_to_one_graph(OpNodeKind::Normal),
        &zero_to_one_graph(OpNodeKind::RetDestPush),
        r#"
            A source:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              outputs: [v0]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              v0 = op0()
              outputs: [v0]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: []
              op0 ret-dest-push
                inputs: []
                outputs: [v0]
                predecessors: []
              outputs: [v0]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: []
              v0 = op0()
              outputs: [v0]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn equal_when_flippable_inputs_are_reversed() {
    let left = binary_graph(true, false);
    let right = binary_graph(true, true);
    assert_canonicalizes_equal(
        &left,
        &right,
        r#"
            A source:
              inputs: [v0, v1]
              op0 flippable
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: []
              outputs: [v2]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0, v1]
              v2 = op0_f(v0, v1)
              outputs: [v2]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: [v0, v1]
              op0 flippable
                inputs: [v1, v0]
                outputs: [v2]
                predecessors: []
              outputs: [v2]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0, v1]
              v2 = op0_f(v0, v1)
              outputs: [v2]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: true
        "#,
    );

    let canonicalized = canonicalize_graph(&right.0, right.1);
    let operation = canonicalized.canonical_op_ids().next().unwrap();
    assert!(canonicalized.first_two_inputs_swapped(operation));
}

#[test]
fn not_equal_when_unflippable_inputs_are_reversed() {
    assert_canonicalizes_different(
        &binary_graph(false, false),
        &binary_graph(false, true),
        r#"
            A source:
              inputs: [v0, v1]
              op0 normal
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: []
              outputs: [v2]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0, v1]
              v2 = op0(v0, v1)
              outputs: [v2]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: [v0, v1]
              op0 normal
                inputs: [v1, v0]
                outputs: [v2]
                predecessors: []
              outputs: [v2]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0, v1]
              v2 = op0(v1, v0)
              outputs: [v2]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_necessary_ordering_differs() {
    assert_canonicalizes_different(
        &ordering_graph(false),
        &ordering_graph(true),
        r#"
            A source:
              inputs: []
              op0 normal
                inputs: []
                outputs: []
                predecessors: []
              op1 normal
                inputs: []
                outputs: []
                predecessors: []
              outputs: []
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              op0()
              op1()
              outputs: []
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false

            B source:
              inputs: []
              op0 normal
                inputs: []
                outputs: []
                predecessors: []
              op1 normal
                inputs: []
                outputs: []
                predecessors: [op0]
              outputs: []
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: []
              op0()
              op1() ; after: [op0]
              outputs: []
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_finalization_differs() {
    assert_canonicalizes_different(
        &empty_graph(BlockFinalization::ShuffleToOutputs),
        &empty_graph(BlockFinalization::LastOpTerminates),
        r#"
            A source:
              inputs: []
              outputs: []
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              outputs: []
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:

            B source:
              inputs: []
              outputs: []
              finalization: last-op-terminates
            B canonical graph:
              inputs: []
              outputs: []
            B canonical metadata:
              finalization: last-op-terminates
              witness:
        "#,
    );
}

#[test]
fn equal_when_an_ordering_edge_is_transitively_redundant() {
    assert_canonicalizes_equal(
        &transitive_ordering_graph(false),
        &transitive_ordering_graph(true),
        r#"
            A source:
              inputs: []
              op0 normal
                inputs: []
                outputs: []
                predecessors: []
              op1 normal
                inputs: []
                outputs: []
                predecessors: [op0]
              op2 normal
                inputs: []
                outputs: []
                predecessors: [op1]
              outputs: []
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              op0()
              op1() ; after: [op0]
              op2() ; after: [op1]
              outputs: []
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false

            B source:
              inputs: []
              op0 normal
                inputs: []
                outputs: []
                predecessors: []
              op1 normal
                inputs: []
                outputs: []
                predecessors: [op0]
              op2 normal
                inputs: []
                outputs: []
                predecessors: [op1]
              outputs: []
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: []
              op0()
              op1() ; after: [op0]
              op2() ; after: [op1]
              outputs: []
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn data_producers_are_not_repeated_as_effect_predecessors() {
    assert_canonicalizes_to(
        &data_dependency_graph(),
        r#"
            Graph source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              op1 normal
                inputs: [v1]
                outputs: [v2]
                predecessors: [op0]
              outputs: []
              finalization: shuffle-to-outputs
            Graph canonical graph:
              inputs: [v0]
              v1 = op0(v0)
              v2 = op1(v1)
              outputs: []
            Graph canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn equal_when_effect_ties_are_reordered() {
    assert_canonicalizes_equal(
        &effect_tie_graph(false),
        &effect_tie_graph(true),
        r#"
            A source:
              inputs: []
              op0 normal
                inputs: []
                outputs: []
                predecessors: []
              op1 normal
                inputs: []
                outputs: []
                predecessors: []
              op2 normal
                inputs: []
                outputs: [v0]
                predecessors: [op0]
              op3 normal
                inputs: []
                outputs: []
                predecessors: [op1]
              outputs: [v0]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              op0()
              op1()
              op2() ; after: [op1]
              v0 = op3() ; after: [op0]
              outputs: [v0]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
                op2: source op3, first-two-inputs-swapped: false
                op3: source op2, first-two-inputs-swapped: false

            B source:
              inputs: []
              op0 normal
                inputs: []
                outputs: []
                predecessors: []
              op1 normal
                inputs: []
                outputs: []
                predecessors: []
              op2 normal
                inputs: []
                outputs: [v0]
                predecessors: [op1]
              op3 normal
                inputs: []
                outputs: []
                predecessors: [op0]
              outputs: [v0]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: []
              op0()
              op1()
              op2() ; after: [op1]
              v0 = op3() ; after: [op0]
              outputs: [v0]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op1, first-two-inputs-swapped: false
                op1: source op0, first-two-inputs-swapped: false
                op2: source op3, first-two-inputs-swapped: false
                op3: source op2, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_flippability_differs() {
    assert_canonicalizes_different(
        &binary_graph(false, false),
        &binary_graph(true, false),
        r#"
            A source:
              inputs: [v0, v1]
              op0 normal
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: []
              outputs: [v2]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0, v1]
              v2 = op0(v0, v1)
              outputs: [v2]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: [v0, v1]
              op0 flippable
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: []
              outputs: [v2]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0, v1]
              v2 = op0_f(v0, v1)
              outputs: [v2]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_input_count_differs() {
    assert_canonicalizes_different(
        &empty_graph_with_inputs(0, BlockFinalization::ShuffleToOutputs),
        &empty_graph_with_inputs(1, BlockFinalization::ShuffleToOutputs),
        r#"
            A source:
              inputs: []
              outputs: []
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              outputs: []
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:

            B source:
              inputs: [v0]
              outputs: []
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0]
              outputs: []
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
        "#,
    );
}

#[test]
fn not_equal_when_operation_count_differs() {
    assert_canonicalizes_different(
        &empty_graph(BlockFinalization::ShuffleToOutputs),
        &output_count_graph(0),
        r#"
            A source:
              inputs: []
              outputs: []
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              outputs: []
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:

            B source:
              inputs: []
              op0 normal
                inputs: []
                outputs: []
                predecessors: []
              outputs: []
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: []
              op0()
              outputs: []
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_operation_input_count_differs() {
    assert_canonicalizes_different(
        &input_count_graph(false),
        &input_count_graph(true),
        r#"
            A source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              outputs: [v1]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0]
              v1 = op0(v0)
              outputs: [v1]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: [v0]
              op0 normal
                inputs: [v0, v0]
                outputs: [v1]
                predecessors: []
              outputs: [v1]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0]
              v1 = op0(v0, v0)
              outputs: [v1]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_operation_output_count_differs() {
    assert_canonicalizes_different(
        &output_count_graph(1),
        &output_count_graph(2),
        r#"
            A source:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              outputs: []
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              v0 = op0()
              outputs: []
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0, v1]
                predecessors: []
              outputs: []
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: []
              [v0, v1] = op0()
              outputs: []
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_output_fifo_order_differs() {
    assert_canonicalizes_different(
        &output_order_graph(false),
        &output_order_graph(true),
        r#"
            A source:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0, v1]
                predecessors: []
              outputs: [v0, v1]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              [v0, v1] = op0()
              outputs: [v0, v1]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0, v1]
                predecessors: []
              outputs: [v1, v0]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: []
              [v0, v1] = op0()
              outputs: [v1, v0]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_output_fifo_multiplicity_differs() {
    assert_canonicalizes_different(
        &output_multiplicity_graph(false),
        &output_multiplicity_graph(true),
        r#"
            A source:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              outputs: [v0]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: []
              v0 = op0()
              outputs: [v0]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              outputs: [v0, v0]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: []
              v0 = op0()
              outputs: [v0, v0]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn equal_when_reordered_multi_output_producers_have_distinct_uses() {
    assert_canonicalizes_equal(
        &multi_output_graph(false, false),
        &multi_output_graph(true, false),
        r#"
            A source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1, v2]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v3, v4]
                predecessors: []
              op2 normal
                inputs: [v1, v4]
                outputs: [v5]
                predecessors: [op0, op1]
              outputs: [v2, v5, v3]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0]
              [v1, v2] = op0(v0)
              [v3, v4] = op1(v0)
              v5 = op2(v3, v2)
              outputs: [v4, v5, v1]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op1, first-two-inputs-swapped: false
                op1: source op0, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false

            B source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1, v2]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v3, v4]
                predecessors: []
              op2 normal
                inputs: [v3, v2]
                outputs: [v5]
                predecessors: [op0, op1]
              outputs: [v4, v5, v1]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0]
              [v1, v2] = op0(v0)
              [v3, v4] = op1(v0)
              v5 = op2(v3, v2)
              outputs: [v4, v5, v1]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn not_equal_when_multi_output_roles_are_exchanged() {
    assert_canonicalizes_different(
        &multi_output_graph(false, false),
        &multi_output_graph(false, true),
        r#"
            A source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1, v2]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v3, v4]
                predecessors: []
              op2 normal
                inputs: [v1, v4]
                outputs: [v5]
                predecessors: [op0, op1]
              outputs: [v2, v5, v3]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0]
              [v1, v2] = op0(v0)
              [v3, v4] = op1(v0)
              v5 = op2(v3, v2)
              outputs: [v4, v5, v1]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op1, first-two-inputs-swapped: false
                op1: source op0, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false

            B source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1, v2]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v3, v4]
                predecessors: []
              op2 normal
                inputs: [v2, v3]
                outputs: [v5]
                predecessors: [op0, op1]
              outputs: [v2, v5, v3]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0]
              [v1, v2] = op0(v0)
              [v3, v4] = op1(v0)
              v5 = op2(v4, v1)
              outputs: [v4, v5, v1]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op1, first-two-inputs-swapped: false
                op1: source op0, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn equal_when_output_positions_distinguish_tied_operations() {
    assert_canonicalizes_equal(
        &tied_output_position_graph(false),
        &tied_output_position_graph(true),
        r#"
            A source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v2]
                predecessors: []
              outputs: [v1, v2]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0]
              v1 = op0(v0)
              v2 = op1(v0)
              outputs: [v1, v2]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false

            B source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v2]
                predecessors: []
              outputs: [v2, v1]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0]
              v1 = op0(v0)
              v2 = op1(v0)
              outputs: [v1, v2]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op1, first-two-inputs-swapped: false
                op1: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn equal_when_consumer_operand_positions_distinguish_tied_operations() {
    assert_canonicalizes_equal(
        &tied_operand_position_graph(false),
        &tied_operand_position_graph(true),
        r#"
            A source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v2]
                predecessors: []
              op2 normal
                inputs: [v1, v2]
                outputs: [v3]
                predecessors: [op0, op1]
              outputs: [v3]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0]
              v1 = op0(v0)
              v2 = op1(v0)
              v3 = op2(v1, v2)
              outputs: [v3]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false

            B source:
              inputs: [v0]
              op0 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v2]
                predecessors: []
              op2 normal
                inputs: [v2, v1]
                outputs: [v3]
                predecessors: [op0, op1]
              outputs: [v3]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0]
              v1 = op0(v0)
              v2 = op1(v0)
              v3 = op2(v1, v2)
              outputs: [v3]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op1, first-two-inputs-swapped: false
                op1: source op0, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn equal_when_only_the_first_two_flippable_inputs_are_reversed() {
    assert_canonicalizes_equal(
        &ternary_flippable_graph([0, 1, 2]),
        &ternary_flippable_graph([1, 0, 2]),
        r#"
            A source:
              inputs: [v0, v1, v2]
              op0 flippable
                inputs: [v0, v1, v2]
                outputs: [v3]
                predecessors: []
              outputs: [v3]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0, v1, v2]
              v3 = op0_f(v0, v1, v2)
              outputs: [v3]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: [v0, v1, v2]
              op0 flippable
                inputs: [v1, v0, v2]
                outputs: [v3]
                predecessors: []
              outputs: [v3]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0, v1, v2]
              v3 = op0_f(v0, v1, v2)
              outputs: [v3]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: true
        "#,
    );
}

#[test]
fn not_equal_when_a_later_flippable_input_is_reordered() {
    assert_canonicalizes_different(
        &ternary_flippable_graph([0, 1, 2]),
        &ternary_flippable_graph([0, 2, 1]),
        r#"
            A source:
              inputs: [v0, v1, v2]
              op0 flippable
                inputs: [v0, v1, v2]
                outputs: [v3]
                predecessors: []
              outputs: [v3]
              finalization: shuffle-to-outputs
            A canonical graph:
              inputs: [v0, v1, v2]
              v3 = op0_f(v0, v1, v2)
              outputs: [v3]
            A canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false

            B source:
              inputs: [v0, v1, v2]
              op0 flippable
                inputs: [v0, v2, v1]
                outputs: [v3]
                predecessors: []
              outputs: [v3]
              finalization: shuffle-to-outputs
            B canonical graph:
              inputs: [v0, v1, v2]
              v3 = op0_f(v0, v2, v1)
              outputs: [v3]
            B canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
fn does_not_mark_equal_flippable_inputs_as_swapped() {
    assert_canonicalizes_to(
        &repeated_flippable_input_graph(),
        r#"
            Graph source:
              inputs: [v0]
              op0 flippable
                inputs: [v0, v0]
                outputs: [v1]
                predecessors: []
              outputs: [v1]
              finalization: shuffle-to-outputs
            Graph canonical graph:
              inputs: [v0]
              v1 = op0_f(v0, v0)
              outputs: [v1]
            Graph canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
        "#,
    );
}

#[test]
#[should_panic(expected = "flippable operation has fewer than two inputs")]
fn rejects_a_flippable_operation_with_fewer_than_two_inputs() {
    let mut graph = OpGraphBuilder::with_capacity(1, 1);
    let input = graph.push_input_value();
    let mut graph = graph.end_inputs_begin_ops();
    let mut operation = graph.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
    operation.add_input(input);
    let graph = graph.end_ops_begin_end_stack().finish();
    canonicalize_graph(&graph, BlockFinalization::ShuffleToOutputs);
}

#[test]
fn nontrivial_key_has_stable_versioned_hex_display() {
    let graph = kitchen_sink_graph();
    let key = assert_canonicalizes_to(
        &graph,
        r#"
            Graph source:
              inputs: [v0, v1]
              op0 normal
                inputs: [v0]
                outputs: [v2, v3]
                predecessors: []
              op1 flippable
                inputs: [v1, v2]
                outputs: [v4]
                predecessors: [op0]
              op2 normal
                inputs: [v3]
                outputs: []
                predecessors: [op1]
              outputs: [v4, v3, v4]
              finalization: shuffle-to-outputs
            Graph canonical graph:
              inputs: [v0, v1]
              [v2, v3] = op0(v0)
              v4 = op1_f(v1, v2)
              op2(v3) ; after: [op1]
              outputs: [v4, v3, v4]
            Graph canonical metadata:
              finalization: shuffle-to-outputs
              witness:
                op0: source op0, first-two-inputs-swapped: false
                op1: source op1, first-two-inputs-swapped: false
                op2: source op2, first-two-inputs-swapped: false
        "#,
    );
    assert_eq!(
        key.to_string(),
        "ssb1:79f5da503e4d01fde597ab18a07c3200219777bbd122132e6d9a749f5c9969b4"
    );
}

#[test]
fn canonical_keys_are_stable_across_valid_topological_orders() {
    let graphs = [
        permuted_tied_graph(false, false),
        permuted_tied_graph(true, false),
        permuted_tied_graph(false, true),
        permuted_tied_graph(true, true),
    ];
    let mut graphs = graphs.iter();
    let (first, finalization) = graphs.next().unwrap();
    let expected = key(first, *finalization);
    for (graph, finalization) in graphs {
        assert_eq!(key(graph, *finalization), expected);
    }
}

#[test]
fn key_has_versioned_hex_display() {
    let (graph, finalization) = empty_graph(BlockFinalization::ShuffleToOutputs);

    assert_eq!(
        key(&graph, finalization).to_string(),
        "ssb1:105c3a3c4eade43a0d32470e29c3fde6612c883f20b1d7514299b2ba8d2f9d87"
    );
}
