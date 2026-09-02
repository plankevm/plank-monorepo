use crate::{
    database::SourceBlock,
    graph::{Graph, OperationId, ValueId},
};
use plank_core::{DenseIndexSet, Idx, IndexVec, newtype_index};
use sir_stack_scheduling_common::{
    BlockFinalization, RepresentativeSchedule, RepresentativeStackOp,
};
use std::fmt::Write;

newtype_index! {
    struct SpillSlotId;
}

pub fn source_blocks(source_blocks: &[SourceBlock]) -> String {
    let mut output = format!("source blocks ({}):", source_blocks.len());
    for source in source_blocks {
        write!(output, "\n  {}: bb{}", source.file, source.block_id).unwrap();
    }
    output
}

pub fn graph(graph: &Graph) -> String {
    let mut output = String::new();
    writeln!(output, "inputs: {}", values(graph.input_values())).unwrap();
    for operation_id in graph.operation_ids() {
        let operation = graph.operation(operation_id);
        let outputs = graph.operation_outputs(operation).collect::<Box<[_]>>();
        match outputs.as_ref() {
            [] => {}
            [output_value] => write!(output, "v{output_value} = ").unwrap(),
            outputs => write!(output, "{} = ", values(outputs.iter().copied())).unwrap(),
        }
        write!(output, "op{operation_id}").unwrap();
        if operation.flippable {
            output.push_str("_f");
        }
        write!(output, "({})", value_names(graph.operation_inputs(operation).iter().copied()))
            .unwrap();
        let effect_predecessors = graph.operation_effect_predecessors(operation);
        if !effect_predecessors.is_empty() {
            write!(output, " ; after: [{}]", operation_names(effect_predecessors.iter().copied()))
                .unwrap();
        }
        output.push('\n');
    }
    write!(output, "outputs: {}", values(graph.outputs_fifo().iter().copied())).unwrap();
    output
}

pub fn schedule(graph: &Graph, schedule: &RepresentativeSchedule) -> Result<String, String> {
    let mut stack = Vec::with_capacity(graph.total_values());
    for input in graph.input_values().rev() {
        stack.push(input);
    }
    let mut spills = IndexVec::<SpillSlotId, ValueId>::new();
    let mut completed =
        DenseIndexSet::<OperationId>::with_capacity_in_bits(graph.operation_ids().count());
    let mut steps = vec![Step::new("; start:", &stack)];

    for &scheduled in schedule.0.iter() {
        let action = apply(scheduled, graph, &mut stack, &mut spills, &mut completed)?;
        steps.push(Step::new(action, &stack));
    }

    if graph.finalization == BlockFinalization::ShuffleToOutputs {
        let actual = stack.iter().rev().copied().collect::<Box<[_]>>();
        if actual.as_ref() != graph.outputs_fifo() {
            return Err(format!(
                "schedule ends with {}, expected {}",
                values(actual),
                values(graph.outputs_fifo().iter().copied())
            ));
        }
    }

    let action_width = steps.iter().map(|step| step.action.len()).max().unwrap_or(0);
    let stack_width = steps.iter().map(|step| step.stack.len()).max().unwrap_or(0);
    let mut output = String::new();
    for (index, step) in steps.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        write!(output, "{:<action_width$}  {:>stack_width$}", step.action, step.stack).unwrap();
    }
    Ok(output)
}

struct Step {
    action: String,
    stack: String,
}

impl Step {
    fn new(action: impl Into<String>, stack: &[ValueId]) -> Self {
        Self { action: action.into(), stack: values(stack.iter().rev().copied()) }
    }
}

fn apply(
    scheduled: RepresentativeStackOp,
    graph: &Graph,
    stack: &mut Vec<ValueId>,
    spills: &mut IndexVec<SpillSlotId, ValueId>,
    completed: &mut DenseIndexSet<OperationId>,
) -> Result<String, String> {
    match scheduled {
        RepresentativeStackOp::Swap { depth } => {
            let target = depth_index(stack, depth)?;
            let top = stack.len().checked_sub(1).expect("depth check ensures a top value");
            stack.swap(top, target);
            Ok(format!("swap{depth}"))
        }
        RepresentativeStackOp::Dup { depth } => {
            let target = depth_index(stack, depth)?;
            stack.push(stack[target]);
            let evm_depth = depth.checked_add(1).expect("dup display depth overflow");
            Ok(format!("dup{evm_depth}"))
        }
        RepresentativeStackOp::Pop => {
            stack.pop().ok_or_else(|| "pop underflowed the stack".to_owned())?;
            Ok("pop".to_owned())
        }
        RepresentativeStackOp::Exchange { first_depth, second_depth } => {
            let first = depth_index(stack, first_depth)?;
            let second = depth_index(stack, second_depth)?;
            stack.swap(first, second);
            Ok(format!("exchange{first_depth},{second_depth}"))
        }
        RepresentativeStackOp::Store { slot } => {
            let value = stack.pop().ok_or_else(|| "store underflowed the stack".to_owned())?;
            let actual_slot = spills.push(value);
            if actual_slot.get() != slot {
                return Err(format!(
                    "store slot {slot} is out of sequence, expected {}",
                    actual_slot.get()
                ));
            }
            Ok(format!("store{slot}"))
        }
        RepresentativeStackOp::Load { slot } => {
            let slot_id = spills
                .iter_idx()
                .find(|candidate| candidate.get() == slot)
                .ok_or_else(|| format!("load refers to missing spill slot {slot}"))?;
            stack.push(spills[slot_id]);
            Ok(format!("load{slot}"))
        }
        RepresentativeStackOp::Op { operation } => {
            apply_operation(graph, stack, completed, operation, false)?;
            Ok(format!("op{operation}"))
        }
        RepresentativeStackOp::Flipped { operation } => {
            apply_operation(graph, stack, completed, operation, true)?;
            Ok(format!("op{operation}'"))
        }
    }
}

fn apply_operation(
    graph: &Graph,
    stack: &mut Vec<ValueId>,
    completed: &mut DenseIndexSet<OperationId>,
    raw_operation: u32,
    flipped: bool,
) -> Result<(), String> {
    let operation_id = graph.resolve_operation(raw_operation)?;
    let operation = graph.operation(operation_id);
    if completed.contains(operation_id) {
        return Err(format!("op{operation_id} is scheduled more than once"));
    }
    if flipped && !operation.flippable {
        return Err(format!("op{operation_id} is flipped but is not flippable"));
    }
    for &predecessor in graph.operation_effect_predecessors(operation) {
        if !completed.contains(predecessor) {
            return Err(format!("op{operation_id} executes before op{predecessor}"));
        }
    }

    let inputs = graph.operation_inputs(operation);
    for position in 0..inputs.len() {
        let expected_position = match position {
            0 if flipped => 1,
            1 if flipped => 0,
            _ => position,
        };
        let expected = inputs
            .get(expected_position)
            .copied()
            .ok_or_else(|| format!("op{operation_id} is flipped but has fewer than two inputs"))?;
        let actual =
            stack.pop().ok_or_else(|| format!("op{operation_id} underflowed the stack"))?;
        if actual != expected {
            return Err(format!("op{operation_id} expected {expected} on top, found {actual}"));
        }
    }
    for output in graph.operation_outputs(operation).rev() {
        stack.push(output);
    }
    completed.add(operation_id);
    Ok(())
}

fn depth_index(stack: &[ValueId], depth: u8) -> Result<usize, String> {
    stack
        .len()
        .checked_sub(usize::from(depth) + 1)
        .ok_or_else(|| format!("stack depth {depth} is out of bounds for {} values", stack.len()))
}

fn values(values: impl IntoIterator<Item = ValueId>) -> String {
    format!("[{}]", value_names(values))
}

fn value_names(values: impl IntoIterator<Item = ValueId>) -> String {
    values.into_iter().map(|value| format!("v{value}")).collect::<Vec<_>>().join(", ")
}

fn operation_names(operations: impl IntoIterator<Item = OperationId>) -> String {
    operations.into_iter().map(|operation| format!("op{operation}")).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use plank_test_utils::dedent_preserve_indent;
    use sir_stack_scheduling_common::{
        BlockFinalization, RepresentativeGraph, RepresentativeOperation,
    };

    #[test]
    fn renders_source_file_and_block_mappings() {
        let source_blocks = [
            SourceBlock { file: "first.sir".to_owned(), block_id: 4 },
            SourceBlock { file: "nested/second.sir".to_owned(), block_id: 17 },
        ];
        let expected = dedent_preserve_indent(
            r#"
            source blocks (2):
              first.sir: bb4
              nested/second.sir: bb17
            "#,
        );

        assert_eq!(crate::render_source_blocks(&source_blocks), expected);
    }

    #[test]
    fn renders_pseudo_sir_and_a_right_aligned_stack_trace() {
        let graph = Graph::from_representative(RepresentativeGraph {
            finalization: BlockFinalization::LastOpTerminates,
            input_count: 3,
            operations: Box::new([
                RepresentativeOperation {
                    inputs_fifo: Box::new([2]),
                    output_count: 1,
                    effect_predecessors: Box::new([]),
                    flippable: false,
                },
                RepresentativeOperation {
                    inputs_fifo: Box::new([0, 3]),
                    output_count: 1,
                    effect_predecessors: Box::new([0]),
                    flippable: true,
                },
            ]),
            outputs_fifo: Box::new([4]),
        })
        .unwrap();
        let best = RepresentativeSchedule(Box::new([
            RepresentativeStackOp::Dup { depth: 2 },
            RepresentativeStackOp::Op { operation: 0 },
            RepresentativeStackOp::Flipped { operation: 1 },
        ]));

        let expected_graph = dedent_preserve_indent(
            r#"
            inputs: [v0, v1, v2]
            v3 = op0(v2)
            v4 = op1_f(v0, v3) ; after: [op0]
            outputs: [v4]
            "#,
        );
        let expected_schedule = dedent_preserve_indent(
            r#"
            ; start:      [v0, v1, v2]
            dup3      [v2, v0, v1, v2]
            op0       [v3, v0, v1, v2]
            op1'          [v4, v1, v2]
            "#,
        );
        assert_eq!(crate::render_graph(&graph), expected_graph);
        assert_eq!(crate::render_schedule(&graph, &best).unwrap(), expected_schedule);
    }
}
