use plank_core::Idx;
use sir_data::{OperationIdx, StaticAllocId};
use sir_stack_scheduling::{
    op_graph::{CanonicalOpId, CanonicalizedBlock, OpGraph, OpNodeKind},
    stack::StackOps,
};
use sir_stack_scheduling_common::{
    BlockFinalization, RepresentativeGraph, RepresentativeOperation, RepresentativeSchedule,
    RepresentativeStackOp,
};
use std::collections::HashMap;

pub fn representative_graph(canonicalized: &CanonicalizedBlock) -> RepresentativeGraph {
    let finalization = if canonicalized.last_op_terminates() {
        BlockFinalization::LastOpTerminates
    } else {
        BlockFinalization::ShuffleToOutputs
    };
    let operations = canonicalized
        .canonical_op_ids()
        .map(|operation| {
            let view = canonicalized.operation(operation);
            RepresentativeOperation {
                inputs_fifo: view.inputs_fifo.iter().map(|value| value.get()).collect(),
                output_count: view.output_count,
                effect_predecessors: view
                    .effect_predecessors
                    .iter()
                    .map(|operation| operation.get())
                    .collect(),
                flippable: view.flippable,
            }
        })
        .collect();
    let outputs_fifo = canonicalized.outputs_fifo().iter().map(|value| value.get()).collect();
    RepresentativeGraph {
        finalization,
        input_count: canonicalized.input_count(),
        operations,
        outputs_fifo,
    }
}

pub fn representative_schedule(
    source: &[StackOps],
    graph: &OpGraph,
    canonicalized: &CanonicalizedBlock,
) -> RepresentativeSchedule {
    let mut spill_slots = HashMap::<StaticAllocId, u32>::new();
    let operations = source
        .iter()
        .map(|&operation| match operation {
            StackOps::Swap(depth) => RepresentativeStackOp::Swap { depth },
            StackOps::Dup(depth) => RepresentativeStackOp::Dup { depth },
            StackOps::Pop => RepresentativeStackOp::Pop,
            StackOps::Exchange(first_depth, second_depth) => {
                RepresentativeStackOp::Exchange { first_depth, second_depth }
            }
            StackOps::Store(allocation) => {
                let slot = u32::try_from(spill_slots.len()).expect("spill slot count overflow");
                assert!(spill_slots.insert(allocation, slot).is_none());
                RepresentativeStackOp::Store { slot }
            }
            StackOps::Load(allocation) => {
                let slot = spill_slots
                    .get(&allocation)
                    .copied()
                    .expect("schedule loads a slot before storing it");
                RepresentativeStackOp::Load { slot }
            }
            StackOps::Op(source_operation) => representative_op(
                source_operation,
                SourceOpKind::Operation { flipped: false },
                graph,
                canonicalized,
            ),
            StackOps::Flipped(source_operation) => representative_op(
                source_operation,
                SourceOpKind::Operation { flipped: true },
                graph,
                canonicalized,
            ),
            StackOps::CallRetPush(source_operation) => representative_op(
                source_operation,
                SourceOpKind::ReturnDestinationPush,
                graph,
                canonicalized,
            ),
        })
        .collect();
    RepresentativeSchedule(operations)
}

#[derive(Clone, Copy)]
enum SourceOpKind {
    Operation { flipped: bool },
    ReturnDestinationPush,
}

fn representative_op(
    source_operation: OperationIdx,
    source_kind: SourceOpKind,
    graph: &OpGraph,
    canonicalized: &CanonicalizedBlock,
) -> RepresentativeStackOp {
    let canonical_operation =
        find_canonical_operation(source_operation, source_kind, graph, canonicalized);
    let witness_swapped = canonicalized.first_two_inputs_swapped(canonical_operation);
    let source_flipped = match source_kind {
        SourceOpKind::Operation { flipped } => flipped,
        SourceOpKind::ReturnDestinationPush => false,
    };
    let operation = canonical_operation.get();
    if source_flipped ^ witness_swapped {
        assert!(canonicalized.operation(canonical_operation).flippable);
        RepresentativeStackOp::Flipped { operation }
    } else {
        RepresentativeStackOp::Op { operation }
    }
}

fn find_canonical_operation(
    source_operation: OperationIdx,
    source_kind: SourceOpKind,
    graph: &OpGraph,
    canonicalized: &CanonicalizedBlock,
) -> CanonicalOpId {
    canonicalized
        .canonical_op_ids()
        .find(|&canonical_operation| {
            let source_node = canonicalized.source_op(canonical_operation);
            match (source_kind, graph.get_op(source_node).kind) {
                (
                    SourceOpKind::Operation { .. },
                    OpNodeKind::Normal(operation) | OpNodeKind::Flippable(operation),
                ) => operation == source_operation,
                (SourceOpKind::ReturnDestinationPush, OpNodeKind::RetDestPush(operation)) => {
                    operation == source_operation
                }
                (SourceOpKind::Operation { .. }, OpNodeKind::RetDestPush(_))
                | (
                    SourceOpKind::ReturnDestinationPush,
                    OpNodeKind::Normal(_) | OpNodeKind::Flippable(_),
                ) => false,
            }
        })
        .expect("scheduled operation is absent from its canonical graph")
}
