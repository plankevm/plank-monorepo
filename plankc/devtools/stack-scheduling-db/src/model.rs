use plank_core::Idx;
use serde::{Deserialize, Serialize};
use sir_data::{OperationIdx, StaticAllocId};
use sir_stack_scheduling::{
    op_graph::{CanonicalOpId, CanonicalizedBlock, OpGraph, OpNodeKind},
    stack::StackOps,
};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentativeGraph {
    pub finalization: BlockFinalization,
    pub input_count: u32,
    pub operations: Box<[RepresentativeOperation]>,
    pub outputs_fifo: Box<[u32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockFinalization {
    ShuffleToOutputs,
    LastOpTerminates,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentativeOperation {
    pub inputs_fifo: Box<[u32]>,
    pub output_count: u32,
    pub effect_predecessors: Box<[u32]>,
    pub flippable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentativeSchedule(pub Box<[RepresentativeStackOp]>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepresentativeStackOp {
    Swap { depth: u8 },
    Dup { depth: u8 },
    Pop,
    Op { operation: u32 },
    Flipped { operation: u32 },
    Exchange { first_depth: u8, second_depth: u8 },
    Store { slot: u32 },
    Load { slot: u32 },
}

impl RepresentativeGraph {
    pub fn from_canonical(canonicalized: &CanonicalizedBlock) -> Self {
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
        Self { finalization, input_count: canonicalized.input_count(), operations, outputs_fifo }
    }
}

impl RepresentativeSchedule {
    pub fn from_source(
        source: &[StackOps],
        graph: &OpGraph,
        canonicalized: &CanonicalizedBlock,
    ) -> Self {
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
        Self(operations)
    }
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

pub fn schedule_gas_cost(schedule: &[StackOps]) -> u64 {
    schedule
        .iter()
        .map(|operation| match operation {
            StackOps::Swap(_) | StackOps::Dup(_) | StackOps::Pop => 3,
            StackOps::Exchange(_, _) => 9,
            StackOps::Store(_) => 9,
            StackOps::Load(_) => 6,
            StackOps::Flipped(_) | StackOps::Op(_) | StackOps::CallRetPush(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_and_schedule_json_round_trip() {
        let graph = RepresentativeGraph {
            finalization: BlockFinalization::ShuffleToOutputs,
            input_count: 2,
            operations: Box::new([RepresentativeOperation {
                inputs_fifo: Box::new([0, 1]),
                output_count: 1,
                effect_predecessors: Box::new([]),
                flippable: true,
            }]),
            outputs_fifo: Box::new([2]),
        };
        let schedule = RepresentativeSchedule(Box::new([
            RepresentativeStackOp::Swap { depth: 1 },
            RepresentativeStackOp::Flipped { operation: 0 },
        ]));

        let graph_text = serde_json::to_string(&graph).unwrap();
        let schedule_text = serde_json::to_string(&schedule).unwrap();

        assert_eq!(serde_json::from_str::<RepresentativeGraph>(&graph_text).unwrap(), graph);
        assert_eq!(
            serde_json::from_str::<RepresentativeSchedule>(&schedule_text).unwrap(),
            schedule
        );
        assert_eq!(
            graph_text,
            r#"{"finalization":"shuffle_to_outputs","input_count":2,"operations":[{"inputs_fifo":[0,1],"output_count":1,"effect_predecessors":[],"flippable":true}],"outputs_fifo":[2]}"#
        );
        assert_eq!(
            schedule_text,
            r#"[{"kind":"swap","depth":1},{"kind":"flipped","operation":0}]"#
        );
    }
}
