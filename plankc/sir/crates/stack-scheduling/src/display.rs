use std::fmt::Write;

use sir_data::{OperationIdx, StaticAllocId};

use crate::{
    BlockFinalization,
    op_graph::{OpGraph, OpNodeId, OpNodeKind, ValueNodeId},
    stack::{ShuffleConfig, StackOps},
    validation::{Replay, ValidationError},
};

pub struct ScheduleTrace {
    pub rendering: String,
    pub error: Option<ValidationError>,
    pub next_spill: Option<StaticAllocId>,
}

pub fn graph(graph: &OpGraph) -> String {
    graph_with_annotations(graph, |_| None)
}

pub fn graph_with_annotations(
    graph: &OpGraph,
    mut annotation: impl FnMut(OpNodeId) -> Option<String>,
) -> String {
    let mut output = String::new();
    writeln!(output, "inputs: {}", values(graph.input_values_fifo().iter())).unwrap();
    for operation_id in graph.op_ids() {
        let operation = graph.get_op(operation_id);
        match operation.outputs_fifo {
            [] => {}
            [output_value] => write!(output, "v{output_value} = ").unwrap(),
            outputs => write!(output, "{} = ", values(outputs.iter().copied())).unwrap(),
        }
        write!(output, "op{operation_id}").unwrap();
        match operation.kind {
            OpNodeKind::Flippable(_) => output.push_str("_f"),
            OpNodeKind::RetDestPush(_) => output.push_str("_ret"),
            OpNodeKind::Normal(_) => {}
        }
        write!(output, "({})", value_names(operation.inputs_fifo.iter().copied())).unwrap();

        let effect_predecessors = graph
            .get_immediate_predecessors(operation_id)
            .filter(|&predecessor| {
                !operation
                    .inputs_fifo
                    .iter()
                    .any(|&input| graph.get_producer(input) == Some(predecessor))
            })
            .collect::<Box<[_]>>();
        if !effect_predecessors.is_empty() {
            write!(output, " ; after: [{}]", operation_names(effect_predecessors.iter().copied()))
                .unwrap();
        }
        if let Some(annotation) = annotation(operation_id) {
            write!(output, " ; {annotation}").unwrap();
        }
        output.push('\n');
    }
    write!(output, "outputs: {}", values(graph.output_values_fifo().iter().copied())).unwrap();
    output
}

pub fn trace(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    first_spill: StaticAllocId,
    operations: &[StackOps],
) -> ScheduleTrace {
    trace_with_action_formatter(
        graph,
        finalization,
        config,
        first_spill,
        operations,
        format_operation,
    )
}

pub fn trace_with_operation_labels(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    first_spill: StaticAllocId,
    operations: &[StackOps],
    mut operation_label: impl FnMut(OperationIdx) -> String,
) -> ScheduleTrace {
    trace_with_action_formatter(graph, finalization, config, first_spill, operations, |operation| {
        match operation {
            StackOps::Op(operation) => operation_label(operation),
            StackOps::Flipped(operation) => format!("[flipped] {}", operation_label(operation)),
            _ => format_operation(operation),
        }
    })
}

fn trace_with_action_formatter(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    first_spill: StaticAllocId,
    operations: &[StackOps],
    mut format_action: impl FnMut(StackOps) -> String,
) -> ScheduleTrace {
    let mut replay = match Replay::new(graph, finalization, config, first_spill) {
        Ok(replay) => replay,
        Err(error) => {
            return ScheduleTrace {
                rendering: String::new(),
                error: Some(error),
                next_spill: None,
            };
        }
    };
    let mut steps = vec![TraceStep::new("; start:", replay.stack_fifo())];
    for (index, &operation) in operations.iter().enumerate() {
        if let Err(error) = replay.apply(index, operation) {
            return ScheduleTrace {
                rendering: render_steps(&steps),
                error: Some(error),
                next_spill: None,
            };
        }
        steps.push(TraceStep::new(format_action(operation), replay.stack_fifo()));
    }
    match replay.finish() {
        Ok(next_spill) => ScheduleTrace {
            rendering: render_steps(&steps),
            error: None,
            next_spill: Some(next_spill),
        },
        Err(error) => {
            ScheduleTrace { rendering: render_steps(&steps), error: Some(error), next_spill: None }
        }
    }
}

fn format_operation(operation: StackOps) -> String {
    match operation {
        StackOps::Swap(depth) => format!("swap{depth}"),
        StackOps::Dup(depth) => format!("dup{}", u16::from(depth) + 1),
        StackOps::Pop => "pop".to_owned(),
        StackOps::Flipped(operation) => format!("op{operation}f"),
        StackOps::Op(operation) => format!("op{operation}"),
        StackOps::CallRetPush(operation) => format!("call_ret_push{operation}"),
        StackOps::Exchange(first, second) => format!("exchange{first},{second}"),
        StackOps::Store(slot) => format!("store{slot}"),
        StackOps::Load(slot) => format!("load{slot}"),
    }
}

fn render_steps(steps: &[TraceStep]) -> String {
    let action_width = steps.iter().map(|step| step.action.len()).max().unwrap_or(0);
    let stack_width = steps.iter().map(|step| step.stack.len()).max().unwrap_or(0);
    let mut output = String::new();
    for (index, step) in steps.iter().enumerate() {
        if index != 0 {
            output.push('\n');
        }
        write!(output, "{:<action_width$}  {:>stack_width$}", step.action, step.stack).unwrap();
    }
    output
}

struct TraceStep {
    action: String,
    stack: String,
}

impl TraceStep {
    fn new(action: impl Into<String>, stack: impl IntoIterator<Item = ValueNodeId>) -> Self {
        Self { action: action.into(), stack: values(stack) }
    }
}

fn values(values: impl IntoIterator<Item = ValueNodeId>) -> String {
    format!("[{}]", value_names(values))
}

fn value_names(values: impl IntoIterator<Item = ValueNodeId>) -> String {
    values.into_iter().map(|value| format!("v{value}")).collect::<Vec<_>>().join(", ")
}

fn operation_names(operations: impl IntoIterator<Item = OpNodeId>) -> String {
    operations.into_iter().map(|operation| format!("op{operation}")).collect::<Vec<_>>().join(", ")
}
