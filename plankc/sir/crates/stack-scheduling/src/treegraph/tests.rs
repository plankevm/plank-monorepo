use std::fmt::Write;

use plank_core::Idx;
use plank_test_utils::dedent_preserve_blank_lines;
use sir_data::OperationIdx;

use super::{TreeGraph, build_tree_graph};
use crate::op_graph::{OpGraph, OpGraphBuilder, OpNodeKind, ValueNodeId};

fn assert_snapshot(input: &OpGraph, expected: &str) {
    let expected = dedent_preserve_blank_lines(expected);
    let trees = build_tree_graph(input);
    let mut out = String::new();
    let formatted = {
        format_graph(&mut out, "input graph", input, None);
        out.push('\n');
        format_graph(&mut out, "tree graph", &trees.graph, Some(&trees));
        out.trim()
    };
    pretty_assertions::assert_str_eq!(formatted, expected.trim());
}

fn format_graph(out: &mut String, heading: &str, graph: &OpGraph, trees: Option<&TreeGraph>) {
    writeln!(out, "{heading}:").unwrap();
    write_values(out, "  inputs", graph.input_values_fifo().iter());
    for operation in graph.op_ids() {
        let op = graph.get_op(operation);
        write!(out, "  op{} {}", operation.get(), kind_name(op.kind)).unwrap();
        if let Some(trees) = trees {
            out.push_str(" = [");
            for (position, step) in trees.original_operations(operation).enumerate() {
                if position != 0 {
                    out.push_str(", ");
                }
                if step.flipped {
                    write!(out, "flipped(op{})", step.operation.get()).unwrap();
                } else {
                    write!(out, "op{}", step.operation.get()).unwrap();
                }
            }
            out.push(']');
        }
        out.push('\n');
        write_values(out, "    inputs", op.inputs_fifo.iter().copied());
        write_values(out, "    outputs", op.outputs_fifo.iter().copied());
        write_operations(out, "    predecessors", graph.displayed_predecessors(operation));
    }
    write_values(out, "  outputs", graph.output_values_fifo().iter().copied());
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

fn write_operations(
    out: &mut String,
    label: &str,
    operations: impl IntoIterator<Item = crate::op_graph::OpNodeId>,
) {
    write!(out, "{label}: [").unwrap();
    for (position, operation) in operations.into_iter().enumerate() {
        if position != 0 {
            out.push_str(", ");
        }
        write!(out, "op{}", operation.get()).unwrap();
    }
    out.push_str("]\n");
}

fn partial_binary_tree() -> OpGraph {
    let mut builder = OpGraphBuilder::with_capacity(3, 4);
    let external = builder.push_input_value();
    let mut builder = builder.end_inputs_begin_ops();

    let high = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let deep = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_input(high);
        op.add_input(deep);
        op.add_input(external);
        op.end_inputs_begin_outputs().add_output()
    };

    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    builder.finish()
}

#[test]
fn constructs_partial_binary_tree_in_depth_first_order() {
    let input = partial_binary_tree();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: [v0]
              op0 normal
                inputs: []
                outputs: [v1]
                predecessors: []
              op1 normal
                inputs: []
                outputs: [v2]
                predecessors: []
              op2 normal
                inputs: [v1, v2, v0]
                outputs: [v3]
                predecessors: [op0, op1]
              outputs: [v3]

            tree graph:
              inputs: [v0]
              op0 normal = [op1, op0, op2]
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              outputs: [v1]
        "#,
    );
}

fn three_operand_tree() -> OpGraph {
    let builder = OpGraphBuilder::with_capacity(4, 4);
    let mut builder = builder.end_inputs_begin_ops();
    let high = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let middle = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let deep = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_input(high);
        op.add_input(middle);
        op.add_input(deep);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    builder.finish()
}

#[test]
fn constructs_three_operand_tree_in_depth_first_order() {
    let input = three_operand_tree();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal
                inputs: []
                outputs: [v1]
                predecessors: []
              op2 normal
                inputs: []
                outputs: [v2]
                predecessors: []
              op3 normal
                inputs: [v0, v1, v2]
                outputs: [v3]
                predecessors: [op0, op1, op2]
              outputs: [v3]

            tree graph:
              inputs: []
              op0 normal = [op2, op1, op0, op3]
                inputs: []
                outputs: [v0]
                predecessors: []
              outputs: [v0]
        "#,
    );
}

fn multi_use_tree_edge() -> OpGraph {
    let builder = OpGraphBuilder::with_capacity(3, 3);
    let mut builder = builder.end_inputs_begin_ops();
    let shared = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let first = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_input(shared);
        op.end_inputs_begin_outputs().add_output()
    };
    let second = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_input(shared);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(first);
    builder.push_end_stack_value(second);
    builder.finish()
}

#[test]
fn materializes_multi_use_operand_as_its_own_virtual_operation() {
    let input = multi_use_tree_edge();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: [op0]
              op2 normal
                inputs: [v0]
                outputs: [v2]
                predecessors: [op0]
              outputs: [v1, v2]

            tree graph:
              inputs: []
              op0 normal = [op0]
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal = [op1]
                inputs: [v0]
                outputs: [v1]
                predecessors: [op0]
              op2 normal = [op2]
                inputs: [v0]
                outputs: [v2]
                predecessors: [op0]
              outputs: [v1, v2]
        "#,
    );
}

fn final_output_tree_edge() -> OpGraph {
    let builder = OpGraphBuilder::with_capacity(2, 2);
    let mut builder = builder.end_inputs_begin_ops();
    let preserved = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let consumed = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_input(preserved);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(preserved);
    builder.push_end_stack_value(consumed);
    builder.finish()
}

#[test]
fn materializes_an_operand_that_is_also_a_final_output() {
    let input = final_output_tree_edge();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: [op0]
              outputs: [v0, v1]

            tree graph:
              inputs: []
              op0 normal = [op0]
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal = [op1]
                inputs: [v0]
                outputs: [v1]
                predecessors: [op0]
              outputs: [v0, v1]
        "#,
    );
}

#[test]
fn preserves_flippability_when_neither_leading_operand_is_folded() {
    let mut builder = OpGraphBuilder::with_capacity(1, 3);
    let high = builder.push_input_value();
    let deep = builder.push_input_value();
    let mut builder = builder.end_inputs_begin_ops();
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
        op.add_input(high);
        op.add_input(deep);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    let input = builder.finish();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: [v0, v1]
              op0 flippable
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: []
              outputs: [v2]

            tree graph:
              inputs: [v0, v1]
              op0 flippable = [op0]
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: []
              outputs: [v2]
        "#,
    );
}

#[test]
fn removes_flippability_when_the_first_leading_operand_is_folded() {
    let mut builder = OpGraphBuilder::with_capacity(2, 3);
    let deep = builder.push_input_value();
    let mut builder = builder.end_inputs_begin_ops();
    let high = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
        op.add_input(high);
        op.add_input(deep);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    let input = builder.finish();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: [v0]
              op0 normal
                inputs: []
                outputs: [v1]
                predecessors: []
              op1 flippable
                inputs: [v1, v0]
                outputs: [v2]
                predecessors: [op0]
              outputs: [v2]

            tree graph:
              inputs: [v0]
              op0 normal = [op0, op1]
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              outputs: [v1]
        "#,
    );
}

#[test]
fn removes_flippability_when_the_second_leading_operand_is_folded() {
    let mut builder = OpGraphBuilder::with_capacity(2, 3);
    let high = builder.push_input_value();
    let mut builder = builder.end_inputs_begin_ops();
    let deep = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
        op.add_input(high);
        op.add_input(deep);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    let input = builder.finish();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: [v0]
              op0 normal
                inputs: []
                outputs: [v1]
                predecessors: []
              op1 flippable
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: [op0]
              outputs: [v2]

            tree graph:
              inputs: [v0]
              op0 normal = [op0, flipped(op1)]
                inputs: [v0]
                outputs: [v1]
                predecessors: []
              outputs: [v1]
        "#,
    );
}

#[test]
fn folding_both_leading_operands_removes_flippability_while_preserving_internal_flip() {
    let builder = OpGraphBuilder::with_capacity(3, 3);
    let mut builder = builder.end_inputs_begin_ops();
    let (high_op, high) = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        let id = op.id();
        (id, op.end_inputs_begin_outputs().add_output())
    };
    let deep = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_predecessor(high_op);
        op.end_inputs_begin_outputs().add_output()
    };
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
        op.add_input(high);
        op.add_input(deep);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    let input = builder.finish();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal
                inputs: []
                outputs: [v1]
                predecessors: [op0]
              op2 flippable
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: [op1]
              outputs: [v2]

            tree graph:
              inputs: []
              op0 normal = [op0, op1, flipped(op2)]
                inputs: []
                outputs: [v0]
                predecessors: []
              outputs: [v0]
        "#,
    );
}

#[test]
fn folds_only_the_viable_leading_operand_when_both_orders_are_interposed() {
    let input = {
        let builder = OpGraphBuilder::with_capacity(4, 3);
        let mut builder = builder.end_inputs_begin_ops();
        let (first_operation, first) = {
            let operation = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            let id = operation.id();
            (id, operation.end_inputs_begin_outputs().add_output())
        };
        let interposed = {
            let mut operation = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            operation.add_predecessor(first_operation);
            let id = operation.id();
            let _operation = operation.end_inputs_begin_outputs();
            id
        };
        let second = {
            let mut operation = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            operation.add_predecessor(interposed);
            operation.end_inputs_begin_outputs().add_output()
        };
        let output = {
            let mut operation = builder.begin_op(OpNodeKind::Flippable(OperationIdx::ZERO));
            operation.add_input(first);
            operation.add_input(second);
            operation.end_inputs_begin_outputs().add_output()
        };
        let mut builder = builder.end_ops_begin_end_stack();
        builder.push_end_stack_value(output);
        builder.finish()
    };
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal
                inputs: []
                outputs: []
                predecessors: [op0]
              op2 normal
                inputs: []
                outputs: [v1]
                predecessors: [op1]
              op3 flippable
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: [op2]
              outputs: [v2]

            tree graph:
              inputs: []
              op0 normal = [op0]
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal = [op1]
                inputs: []
                outputs: []
                predecessors: [op0]
              op2 normal = [op2, flipped(op3)]
                inputs: [v0]
                outputs: [v1]
                predecessors: [op1]
              outputs: [v1]
        "#,
    );
}

fn effect_split_tree() -> OpGraph {
    let builder = OpGraphBuilder::with_capacity(3, 3);
    let mut builder = builder.end_inputs_begin_ops();
    let (high_op, high) = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        let id = op.id();
        (id, op.end_inputs_begin_outputs().add_output())
    };
    let deep = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_predecessor(high_op);
        op.end_inputs_begin_outputs().add_output()
    };
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_input(high);
        op.add_input(deep);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    builder.finish()
}

#[test]
fn splits_non_flippable_effect_conflict() {
    let input = effect_split_tree();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal
                inputs: []
                outputs: [v1]
                predecessors: [op0]
              op2 normal
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: [op1]
              outputs: [v2]

            tree graph:
              inputs: []
              op0 normal = [op0]
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal = [op1]
                inputs: []
                outputs: [v1]
                predecessors: [op0]
              op2 normal = [op2]
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: [op1]
              outputs: [v2]
        "#,
    );
}

fn effect_interposed_tree() -> OpGraph {
    let builder = OpGraphBuilder::with_capacity(3, 2);
    let mut builder = builder.end_inputs_begin_ops();
    let (producer, value) = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        let id = op.id();
        (id, op.end_inputs_begin_outputs().add_output())
    };
    let interposed = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_predecessor(producer);
        let id = op.id();
        let _op = op.end_inputs_begin_outputs();
        id
    };
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_predecessor(interposed);
        op.add_input(value);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    builder.finish()
}

#[test]
fn splits_tree_when_an_effect_is_interposed() {
    let input = effect_interposed_tree();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal
                inputs: []
                outputs: []
                predecessors: [op0]
              op2 normal
                inputs: [v0]
                outputs: [v1]
                predecessors: [op1]
              outputs: [v1]

            tree graph:
              inputs: []
              op0 normal = [op0]
                inputs: []
                outputs: [v0]
                predecessors: []
              op1 normal = [op1]
                inputs: []
                outputs: []
                predecessors: [op0]
              op2 normal = [op2]
                inputs: [v0]
                outputs: [v1]
                predecessors: [op1]
              outputs: [v1]
        "#,
    );
}

fn repeated_and_multi_output_operands() -> OpGraph {
    let builder = OpGraphBuilder::with_capacity(3, 4);
    let mut builder = builder.end_inputs_begin_ops();
    let (first, second) = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        let mut op = op.end_inputs_begin_outputs();
        (op.add_output(), op.add_output())
    };
    let repeated = {
        let op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.end_inputs_begin_outputs().add_output()
    };
    let output = {
        let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        op.add_input(first);
        op.add_input(second);
        op.add_input(repeated);
        op.add_input(repeated);
        op.end_inputs_begin_outputs().add_output()
    };
    let mut builder = builder.end_ops_begin_end_stack();
    builder.push_end_stack_value(output);
    builder.finish()
}

#[test]
fn does_not_absorb_multi_output_or_repeated_operands() {
    let input = repeated_and_multi_output_operands();
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: [v0, v1]
                predecessors: []
              op1 normal
                inputs: []
                outputs: [v2]
                predecessors: []
              op2 normal
                inputs: [v0, v1, v2, v2]
                outputs: [v3]
                predecessors: [op0, op1]
              outputs: [v3]

            tree graph:
              inputs: []
              op0 normal = [op0]
                inputs: []
                outputs: [v0, v1]
                predecessors: []
              op1 normal = [op1]
                inputs: []
                outputs: [v2]
                predecessors: []
              op2 normal = [op2]
                inputs: [v0, v1, v2, v2]
                outputs: [v3]
                predecessors: [op0, op1]
              outputs: [v3]
        "#,
    );
}

#[test]
fn folds_operands_with_a_common_predecessor() {
    let input = {
        let builder = OpGraphBuilder::with_capacity(4, 3);
        let mut builder = builder.end_inputs_begin_ops();
        let common = {
            let operation = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            let id = operation.id();
            let _operation = operation.end_inputs_begin_outputs();
            id
        };
        let high = {
            let mut operation = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            operation.add_predecessor(common);
            operation.end_inputs_begin_outputs().add_output()
        };
        let deep = {
            let mut operation = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            operation.add_predecessor(common);
            operation.end_inputs_begin_outputs().add_output()
        };
        let output = {
            let mut operation = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            operation.add_input(high);
            operation.add_input(deep);
            operation.end_inputs_begin_outputs().add_output()
        };
        let mut builder = builder.end_ops_begin_end_stack();
        builder.push_end_stack_value(output);
        builder.finish()
    };
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: []
              op0 normal
                inputs: []
                outputs: []
                predecessors: []
              op1 normal
                inputs: []
                outputs: [v0]
                predecessors: [op0]
              op2 normal
                inputs: []
                outputs: [v1]
                predecessors: [op0]
              op3 normal
                inputs: [v0, v1]
                outputs: [v2]
                predecessors: [op1, op2]
              outputs: [v2]

            tree graph:
              inputs: []
              op0 normal = [op0]
                inputs: []
                outputs: []
                predecessors: []
              op1 normal = [op2, op1, op3]
                inputs: []
                outputs: [v0]
                predecessors: [op0]
              outputs: [v0]
        "#,
    );
}

#[test]
fn does_not_fold_partial_operand_trees() {
    let input = {
        let mut builder = OpGraphBuilder::with_capacity(3, 7);
        let high_left = builder.push_input_value();
        let high_right = builder.push_input_value();
        let deep_left = builder.push_input_value();
        let deep_right = builder.push_input_value();
        let mut builder = builder.end_inputs_begin_ops();
        let high = {
            let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            op.add_input(high_left);
            op.add_input(high_right);
            op.end_inputs_begin_outputs().add_output()
        };
        let deep = {
            let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            op.add_input(deep_left);
            op.add_input(deep_right);
            op.end_inputs_begin_outputs().add_output()
        };
        let output = {
            let mut op = builder.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
            op.add_input(high);
            op.add_input(deep);
            op.end_inputs_begin_outputs().add_output()
        };
        let mut builder = builder.end_ops_begin_end_stack();
        builder.push_end_stack_value(output);
        builder.finish()
    };
    assert_snapshot(
        &input,
        r#"
            input graph:
              inputs: [v0, v1, v2, v3]
              op0 normal
                inputs: [v0, v1]
                outputs: [v4]
                predecessors: []
              op1 normal
                inputs: [v2, v3]
                outputs: [v5]
                predecessors: []
              op2 normal
                inputs: [v4, v5]
                outputs: [v6]
                predecessors: [op0, op1]
              outputs: [v6]

            tree graph:
              inputs: [v0, v1, v2, v3]
              op0 normal = [op0]
                inputs: [v0, v1]
                outputs: [v4]
                predecessors: []
              op1 normal = [op1]
                inputs: [v2, v3]
                outputs: [v5]
                predecessors: []
              op2 normal = [op2]
                inputs: [v4, v5]
                outputs: [v6]
                predecessors: [op0, op1]
              outputs: [v6]
        "#,
    );
}
