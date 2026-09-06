use std::fmt;

use plank_core::{DenseIndexSet, Idx};
use sir_data::{OperationIdx, StaticAllocId};

use crate::{
    BlockFinalization,
    op_graph::{OpGraph, OpNodeId, OpNodeKind, ValueNodeId},
    stack::{ShuffleConfig, StackOps},
};

const MAX_EVM_STACK_HEIGHT: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    operation_index: Option<usize>,
    message: String,
}

impl ValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self { operation_index: None, message: message.into() }
    }

    fn at(operation_index: usize, message: impl Into<String>) -> Self {
        Self { operation_index: Some(operation_index), message: message.into() }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(index) = self.operation_index {
            write!(formatter, "stack operation {}: ", index + 1)?;
        }
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

pub fn validate(
    graph: &OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    first_spill: StaticAllocId,
    operations: &[StackOps],
) -> Result<StaticAllocId, ValidationError> {
    let mut replay = Replay::new(graph, finalization, config, first_spill)?;
    for (index, &operation) in operations.iter().enumerate() {
        replay.apply(index, operation)?;
    }
    replay.finish()
}

pub(crate) struct Replay<'graph> {
    graph: &'graph OpGraph,
    finalization: BlockFinalization,
    config: ShuffleConfig,
    first_spill: StaticAllocId,
    next_spill: StaticAllocId,
    stack: Vec<ValueNodeId>,
    spills: Vec<ValueNodeId>,
    completed: DenseIndexSet<OpNodeId>,
}

impl<'graph> Replay<'graph> {
    pub(crate) fn new(
        graph: &'graph OpGraph,
        finalization: BlockFinalization,
        config: ShuffleConfig,
        first_spill: StaticAllocId,
    ) -> Result<Self, ValidationError> {
        let stack = graph.input_values_fifo().iter().rev().collect::<Vec<_>>();
        if stack.len() > MAX_EVM_STACK_HEIGHT {
            return Err(ValidationError::new(format!(
                "initial stack height {} exceeds the EVM limit of {MAX_EVM_STACK_HEIGHT}",
                stack.len()
            )));
        }
        Ok(Self {
            graph,
            finalization,
            config,
            first_spill,
            next_spill: first_spill,
            stack,
            spills: Vec::new(),
            completed: DenseIndexSet::with_capacity_in_bits(graph.total_ops() as usize),
        })
    }

    pub(crate) fn stack_fifo(&self) -> impl DoubleEndedIterator<Item = ValueNodeId> + '_ {
        self.stack.iter().rev().copied()
    }

    pub(crate) fn apply(
        &mut self,
        index: usize,
        operation: StackOps,
    ) -> Result<(), ValidationError> {
        if !operation.is_valid(self.config) {
            return Err(ValidationError::at(
                index,
                format!("{operation} is not legal for the shuffle configuration"),
            ));
        }
        self.apply_checked(operation).map_err(|message| ValidationError::at(index, message))?;
        if self.stack.len() > MAX_EVM_STACK_HEIGHT {
            return Err(ValidationError::at(
                index,
                format!(
                    "stack height {} exceeds the EVM limit of {MAX_EVM_STACK_HEIGHT}",
                    self.stack.len()
                ),
            ));
        }
        Ok(())
    }

    fn apply_checked(&mut self, operation: StackOps) -> Result<(), String> {
        match operation {
            StackOps::Swap(depth) => {
                let target = self.depth(depth)?;
                let top = self.stack.len().checked_sub(1).expect("validated depth has a top");
                self.stack.swap(top, target);
            }
            StackOps::Dup(depth) => {
                let target = self.depth(depth)?;
                self.stack.push(self.stack[target]);
            }
            StackOps::Pop => {
                self.stack.pop().ok_or_else(|| "pop underflowed the stack".to_owned())?;
            }
            StackOps::Exchange(first_depth, second_depth) => {
                let first = self.depth(first_depth)?;
                let second = self.depth(second_depth)?;
                self.stack.swap(first, second);
            }
            StackOps::Store(slot) => {
                if slot != self.next_spill {
                    return Err(format!(
                        "store {slot} is out of sequence, expected store{}",
                        self.next_spill
                    ));
                }
                let value =
                    self.stack.pop().ok_or_else(|| "store underflowed the stack".to_owned())?;
                self.spills.push(value);
                self.next_spill = self
                    .next_spill
                    .checked_add(1)
                    .ok_or_else(|| "spill allocation ID overflow".to_owned())?;
            }
            StackOps::Load(slot) => {
                let offset = slot
                    .get()
                    .checked_sub(self.first_spill.get())
                    .ok_or_else(|| format!("load refers to missing spill allocation {slot}"))?;
                let value = self
                    .spills
                    .get(offset as usize)
                    .copied()
                    .ok_or_else(|| format!("load refers to missing spill allocation {slot}"))?;
                self.stack.push(value);
            }
            StackOps::Op(operation) => self.apply_operation(operation, false, false)?,
            StackOps::Flipped(operation) => self.apply_operation(operation, true, false)?,
            StackOps::CallRetPush(operation) => self.apply_operation(operation, false, true)?,
        }
        Ok(())
    }

    fn apply_operation(
        &mut self,
        source_operation: OperationIdx,
        flipped: bool,
        return_destination: bool,
    ) -> Result<(), String> {
        let operation_id = self
            .graph
            .op_ids()
            .find(|&candidate| match self.graph.get_op(candidate).kind {
                OpNodeKind::RetDestPush(operation) => {
                    return_destination && operation == source_operation
                }
                OpNodeKind::Normal(operation) | OpNodeKind::Flippable(operation) => {
                    !return_destination && operation == source_operation
                }
            })
            .ok_or_else(|| format!("schedule refers to missing operation {source_operation}"))?;
        let operation = self.graph.get_op(operation_id);
        if self.completed.contains(operation_id) {
            return Err(format!("op{operation_id} is scheduled more than once"));
        }
        if flipped && !matches!(operation.kind, OpNodeKind::Flippable(_)) {
            return Err(format!("op{operation_id} is flipped but is not flippable"));
        }
        if let Some(predecessor) =
            operation.predecessors.iter().find(|&predecessor| !self.completed.contains(predecessor))
        {
            return Err(format!("op{operation_id} executes before op{predecessor}"));
        }

        for position in 0..operation.inputs_fifo.len() {
            let expected_position = match position {
                0 if flipped => 1,
                1 if flipped => 0,
                _ => position,
            };
            let expected =
                operation.inputs_fifo.get(expected_position).copied().ok_or_else(|| {
                    format!("op{operation_id} is flipped but has fewer than two inputs")
                })?;
            let actual = self
                .stack
                .pop()
                .ok_or_else(|| format!("op{operation_id} underflowed the stack"))?;
            if actual != expected {
                return Err(format!(
                    "op{operation_id} expected v{expected} on top, found v{actual}"
                ));
            }
        }
        for &output in operation.outputs_fifo.iter().rev() {
            self.stack.push(output);
        }
        self.completed.add(operation_id);
        Ok(())
    }

    fn depth(&self, depth: u8) -> Result<usize, String> {
        self.stack.len().checked_sub(usize::from(depth) + 1).ok_or_else(|| {
            format!("stack depth {depth} is out of bounds for {} values", self.stack.len())
        })
    }

    pub(crate) fn finish(self) -> Result<StaticAllocId, ValidationError> {
        if let Some(missing) =
            self.graph.op_ids().find(|&operation| !self.completed.contains(operation))
        {
            return Err(ValidationError::new(format!("schedule does not execute op{missing}")));
        }
        if self.finalization == BlockFinalization::ShuffleToOutputs {
            let actual = self.stack_fifo().collect::<Box<[_]>>();
            if actual.as_ref() != self.graph.output_values_fifo() {
                return Err(ValidationError::new(format!(
                    "schedule ends with {}, expected {}",
                    values(actual),
                    values(self.graph.output_values_fifo().iter().copied())
                )));
            }
        }
        Ok(self.next_spill)
    }
}

fn values(values: impl IntoIterator<Item = ValueNodeId>) -> String {
    format!(
        "[{}]",
        values.into_iter().map(|value| format!("v{value}")).collect::<Vec<_>>().join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::op_graph::{OpGraphBuilder, OpNodeKind};

    fn unary_graph() -> OpGraph {
        let mut graph = OpGraphBuilder::with_capacity(1, 2);
        let input = graph.push_input_value();
        let mut graph = graph.end_inputs_begin_ops();
        let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        operation.add_input(input);
        let output = operation.end_inputs_begin_outputs().add_output();
        let mut graph = graph.end_ops_begin_end_stack();
        graph.push_end_stack_value(output);
        graph.finish()
    }

    fn validate_pre_amsterdam(
        graph: &OpGraph,
        operations: &[StackOps],
    ) -> Result<StaticAllocId, ValidationError> {
        validate(
            graph,
            BlockFinalization::ShuffleToOutputs,
            ShuffleConfig::PRE_AMSTERDAM,
            StaticAllocId::ZERO,
            operations,
        )
    }

    #[test]
    fn validates_operations_and_final_stack() {
        assert_eq!(
            validate_pre_amsterdam(&unary_graph(), &[StackOps::Op(OperationIdx::ZERO)]).unwrap(),
            StaticAllocId::ZERO
        );
    }

    #[test]
    fn rejects_an_incorrect_operand() {
        let mut graph = OpGraphBuilder::with_capacity(1, 3);
        let first = graph.push_input_value();
        graph.push_input_value();
        let mut graph = graph.end_inputs_begin_ops();
        let mut operation = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO));
        operation.add_input(first);
        let output = operation.end_inputs_begin_outputs().add_output();
        let mut graph = graph.end_ops_begin_end_stack();
        graph.push_end_stack_value(output);
        let error = validate_pre_amsterdam(
            &graph.finish(),
            &[StackOps::Swap(1), StackOps::Op(OperationIdx::ZERO)],
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "stack operation 2: op0 expected v0 on top, found v1");
    }

    #[test]
    fn rejects_effect_reordering() {
        let graph = OpGraphBuilder::with_capacity(2, 0);
        let mut graph = graph.end_inputs_begin_ops();
        let first =
            graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO)).end_inputs_begin_outputs().id();
        let mut second = graph.begin_op(OpNodeKind::Normal(OperationIdx::ZERO + 1));
        second.add_predecessor(first);
        let _ = second.end_inputs_begin_outputs();
        let graph = graph.end_ops_begin_end_stack().finish();
        let error = validate(
            &graph,
            BlockFinalization::LastOpTerminates,
            ShuffleConfig::PRE_AMSTERDAM,
            StaticAllocId::ZERO,
            &[StackOps::Op(OperationIdx::ZERO + 1), StackOps::Op(OperationIdx::ZERO)],
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "stack operation 1: op1 executes before op0");
    }

    #[test]
    fn rejects_illegal_and_incomplete_schedules() {
        let graph = unary_graph();
        assert_eq!(
            validate_pre_amsterdam(&graph, &[StackOps::Swap(0)]).unwrap_err().to_string(),
            "stack operation 1: Swap(0) is not legal for the shuffle configuration"
        );
        assert_eq!(
            validate_pre_amsterdam(&graph, &[StackOps::Exchange(1, 1)]).unwrap_err().to_string(),
            "stack operation 1: Exchange(1, 1) is not legal for the shuffle configuration"
        );
        assert_eq!(
            validate_pre_amsterdam(&graph, &[]).unwrap_err().to_string(),
            "schedule does not execute op0"
        );
    }

    #[test]
    fn validates_sequential_spills() {
        let graph = unary_graph();
        let operations = [
            StackOps::Store(StaticAllocId::ZERO),
            StackOps::Load(StaticAllocId::ZERO),
            StackOps::Op(OperationIdx::ZERO),
        ];
        assert_eq!(validate_pre_amsterdam(&graph, &operations).unwrap(), StaticAllocId::ZERO + 1);
        assert_eq!(
            validate_pre_amsterdam(&graph, &[StackOps::Store(StaticAllocId::ZERO + 1)])
                .unwrap_err()
                .to_string(),
            "stack operation 1: store 1 is out of sequence, expected store0"
        );
    }
}
