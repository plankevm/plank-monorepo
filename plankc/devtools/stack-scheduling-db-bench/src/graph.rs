use plank_core::{Idx, IndexVec, newtype_index};
use sir_data::{OperationIdx, StaticAllocId};
use sir_stack_scheduling::{
    BlockFinalization, GraphScheduleResult,
    op_graph::{OpGraph, OpGraphBuilder, OpNodeId, OpNodeKind, ValueNodeId},
    stack::StackOps,
};
use sir_stack_scheduling_common::{
    BlockFinalization as RepresentativeFinalization, RepresentativeGraph, RepresentativeSchedule,
    RepresentativeStackOp,
};

newtype_index! {
    struct RepresentativeValueId;
    struct SpillSlotId;
}

pub struct SchedulableGraph {
    pub graph: OpGraph,
    pub finalization: BlockFinalization,
}

pub fn reconstruct(representative: &RepresentativeGraph) -> Result<SchedulableGraph, String> {
    let estimated_values = representative.operations.iter().try_fold(
        usize::try_from(representative.input_count)
            .map_err(|_| "representative input count does not fit usize")?,
        |total, operation| {
            let outputs = usize::try_from(operation.output_count)
                .map_err(|_| "representative output count does not fit usize")?;
            total.checked_add(outputs).ok_or("representative value count overflow")
        },
    )?;
    let mut operation_ids = IndexVec::<OperationIdx, OpNodeId>::new();
    let mut value_ids = IndexVec::<RepresentativeValueId, ValueNodeId>::new();
    let mut graph =
        OpGraphBuilder::with_capacity(representative.operations.len(), estimated_values);
    for _ in 0..representative.input_count {
        value_ids.push(graph.push_input_value());
    }
    let mut graph = graph.end_inputs_begin_ops();

    for representative_operation in &representative.operations {
        let source_operation = operation_ids.next_idx();
        let kind = if representative_operation.flippable {
            OpNodeKind::Flippable(source_operation)
        } else {
            OpNodeKind::Normal(source_operation)
        };
        let mut operation = graph.begin_op(kind);
        for &raw_input in &representative_operation.inputs_fifo {
            operation.add_input(resolve_value(&value_ids, raw_input)?);
        }
        for &raw_predecessor in &representative_operation.effect_predecessors {
            operation.add_predecessor(resolve_operation(&operation_ids, raw_predecessor)?);
        }
        let operation_id = operation.id();
        let mut operation = operation.end_inputs_begin_outputs();
        for _ in 0..representative_operation.output_count {
            value_ids.push(operation.add_output());
        }
        assert_eq!(operation_ids.push(operation_id), source_operation);
    }

    let mut graph = graph.end_ops_begin_end_stack();
    for &raw_output in &representative.outputs_fifo {
        graph.push_end_stack_value(resolve_value(&value_ids, raw_output)?);
    }
    let finalization = match representative.finalization {
        RepresentativeFinalization::ShuffleToOutputs => BlockFinalization::ShuffleToOutputs,
        RepresentativeFinalization::LastOpTerminates => BlockFinalization::LastOpTerminates,
    };
    Ok(SchedulableGraph { graph: graph.finish(), finalization })
}

pub fn representative_schedule(
    result: &GraphScheduleResult,
) -> Result<RepresentativeSchedule, String> {
    let mut spill_allocations = IndexVec::<SpillSlotId, StaticAllocId>::new();
    let operations = result
        .ops
        .iter()
        .map(|&operation| match operation {
            StackOps::Swap(depth) => Ok(RepresentativeStackOp::Swap { depth }),
            StackOps::Dup(depth) => Ok(RepresentativeStackOp::Dup { depth }),
            StackOps::Pop => Ok(RepresentativeStackOp::Pop),
            StackOps::Exchange(first_depth, second_depth) => {
                Ok(RepresentativeStackOp::Exchange { first_depth, second_depth })
            }
            StackOps::Store(allocation) => {
                if spill_allocations.contains(&allocation) {
                    return Err(format!("scheduler stores spill allocation {allocation} twice"));
                }
                let slot = spill_allocations.push(allocation);
                Ok(RepresentativeStackOp::Store { slot: slot.get() })
            }
            StackOps::Load(allocation) => {
                let slot = spill_allocations
                    .enumerate_idx()
                    .find_map(|(slot, &stored)| (stored == allocation).then_some(slot))
                    .ok_or_else(|| {
                        format!("scheduler loads unknown spill allocation {allocation}")
                    })?;
                Ok(RepresentativeStackOp::Load { slot: slot.get() })
            }
            StackOps::Op(operation) | StackOps::CallRetPush(operation) => {
                Ok(RepresentativeStackOp::Op { operation: operation.get() })
            }
            StackOps::Flipped(operation) => {
                Ok(RepresentativeStackOp::Flipped { operation: operation.get() })
            }
        })
        .collect::<Result<Box<[_]>, _>>()?;
    if spill_allocations.len()
        != usize::try_from(result.spill_count).expect("spill count does not fit usize")
    {
        return Err(format!(
            "scheduler reports {} spills but stores {} allocations",
            result.spill_count,
            spill_allocations.len()
        ));
    }
    Ok(RepresentativeSchedule(operations))
}

fn resolve_value(
    values: &IndexVec<RepresentativeValueId, ValueNodeId>,
    raw: u32,
) -> Result<ValueNodeId, String> {
    values
        .enumerate_idx()
        .find_map(|(representative, &value)| (representative.get() == raw).then_some(value))
        .ok_or_else(|| format!("graph refers to missing v{raw}"))
}

fn resolve_operation(
    operations: &IndexVec<OperationIdx, OpNodeId>,
    raw: u32,
) -> Result<OpNodeId, String> {
    operations
        .enumerate_idx()
        .find_map(|(representative, &operation)| (representative.get() == raw).then_some(operation))
        .ok_or_else(|| format!("graph refers to missing op{raw}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sir_stack_scheduling::schedule_graph;
    use sir_stack_scheduling_common::{RepresentativeOperation, RepresentativeStackOp};

    #[test]
    fn reconstructs_and_schedules_a_representative_graph() {
        let representative = RepresentativeGraph {
            finalization: RepresentativeFinalization::ShuffleToOutputs,
            input_count: 1,
            operations: Box::new([RepresentativeOperation {
                inputs_fifo: Box::new([0]),
                output_count: 1,
                effect_predecessors: Box::new([]),
                flippable: false,
            }]),
            outputs_fifo: Box::new([1]),
        };
        let graph = reconstruct(&representative).unwrap();
        let result = schedule_graph(&graph.graph, graph.finalization);

        assert_eq!(
            representative_schedule(&result).unwrap(),
            RepresentativeSchedule(Box::new([RepresentativeStackOp::Op { operation: 0 }]))
        );
        assert_eq!(result.spill_count, 0);
        assert!(!result.candidate_limit_reached);
    }
}
