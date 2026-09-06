use std::collections::HashMap;

use plank_core::Idx;
use sir_data::{OperationIdx, StaticAllocId};

use crate::stack::StackOps;

use super::{CanonicalOpId, CanonicalizedBlock, OpGraph, OpNodeKind};

pub fn canonical_schedule(
    source: &[StackOps],
    graph: &OpGraph,
    canonicalized: &CanonicalizedBlock,
) -> Result<Box<[StackOps]>, String> {
    let mut spill_slots = HashMap::<StaticAllocId, StaticAllocId>::new();
    source
        .iter()
        .map(|&operation| match operation {
            StackOps::Swap(depth) => Ok(StackOps::Swap(depth)),
            StackOps::Dup(depth) => Ok(StackOps::Dup(depth)),
            StackOps::Pop => Ok(StackOps::Pop),
            StackOps::Exchange(first_depth, second_depth) => {
                Ok(StackOps::Exchange(first_depth, second_depth))
            }
            StackOps::Store(allocation) => {
                let raw = u32::try_from(spill_slots.len()).expect("spill slot count overflow");
                let slot = StaticAllocId::try_new(raw).expect("spill slot index overflow");
                if spill_slots.insert(allocation, slot).is_some() {
                    return Err(format!("scheduler stores spill allocation {allocation} twice"));
                }
                Ok(StackOps::Store(slot))
            }
            StackOps::Load(allocation) => {
                let slot = spill_slots.get(&allocation).copied().ok_or_else(|| {
                    format!("scheduler loads unknown spill allocation {allocation}")
                })?;
                Ok(StackOps::Load(slot))
            }
            StackOps::Op(source_operation) => canonical_operation(
                source_operation,
                SourceOpKind::Operation { flipped: false },
                graph,
                canonicalized,
            ),
            StackOps::Flipped(source_operation) => canonical_operation(
                source_operation,
                SourceOpKind::Operation { flipped: true },
                graph,
                canonicalized,
            ),
            StackOps::CallRetPush(source_operation) => canonical_operation(
                source_operation,
                SourceOpKind::ReturnDestinationPush,
                graph,
                canonicalized,
            ),
        })
        .collect()
}

#[derive(Clone, Copy)]
enum SourceOpKind {
    Operation { flipped: bool },
    ReturnDestinationPush,
}

fn canonical_operation(
    source_operation: OperationIdx,
    source_kind: SourceOpKind,
    graph: &OpGraph,
    canonicalized: &CanonicalizedBlock,
) -> Result<StackOps, String> {
    let canonical_operation =
        find_canonical_operation(source_operation, source_kind, graph, canonicalized).ok_or_else(
            || format!("scheduled operation {source_operation} is absent from its graph"),
        )?;
    let witness_swapped = canonicalized.first_two_inputs_swapped(canonical_operation);
    let source_flipped = match source_kind {
        SourceOpKind::Operation { flipped } => flipped,
        SourceOpKind::ReturnDestinationPush => false,
    };
    let operation = OperationIdx::try_from(canonical_operation.idx())
        .map_err(|_| "canonical operation ID does not fit OperationIdx".to_owned())?;
    if source_flipped ^ witness_swapped {
        if !canonicalized.operation(canonical_operation).flippable {
            return Err(format!("canonical operation {canonical_operation} is not flippable"));
        }
        Ok(StackOps::Flipped(operation))
    } else {
        Ok(StackOps::Op(operation))
    }
}

fn find_canonical_operation(
    source_operation: OperationIdx,
    source_kind: SourceOpKind,
    graph: &OpGraph,
    canonicalized: &CanonicalizedBlock,
) -> Option<CanonicalOpId> {
    canonicalized.canonical_op_ids().find(|&canonical_operation| {
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
}
