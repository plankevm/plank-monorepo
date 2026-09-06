use std::fmt;

use plank_core::{DenseIndexSet, Idx, IndexVec, list_of_lists::ListOfLists, newtype_index};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sir_data::{BlockView, OperationIdx};

use crate::BlockFinalization;

use super::{OpGraph, OpGraphBuilder, OpNodeId, OpNodeKind, ValueNodeId};

newtype_index! {
    pub struct CanonicalOpId;
    pub struct CanonicalValueId;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalOperation {
    pub inputs_fifo: Box<[CanonicalValueId]>,
    pub effect_predecessors: Box<[CanonicalOpId]>,
    pub output_count: u32,
    pub flippable: bool,
}

#[derive(Debug, Clone)]
pub struct CanonicalBlock {
    pub finalization: BlockFinalization,
    pub input_count: u32,
    operations: IndexVec<CanonicalOpId, CanonicalOperation>,
    pub outputs_fifo: Box<[CanonicalValueId]>,
}

#[derive(Debug, Clone, Copy)]
struct CanonicalOpWitness {
    source: OpNodeId,
    first_two_inputs_swapped: bool,
}

#[derive(Debug)]
pub struct CanonicalizedBlock {
    canonical: CanonicalBlock,
    witness: IndexVec<CanonicalOpId, CanonicalOpWitness>,
}

impl PartialEq for CanonicalBlock {
    fn eq(&self, other: &Self) -> bool {
        self.finalization == other.finalization
            && self.input_count == other.input_count
            && self.operations.as_raw_slice() == other.operations.as_raw_slice()
            && self.outputs_fifo == other.outputs_fifo
    }
}

impl Eq for CanonicalBlock {}

#[derive(Serialize, Deserialize)]
struct SerializedCanonicalBlock {
    finalization: BlockFinalization,
    input_count: u32,
    operations: Box<[SerializedCanonicalOperation]>,
    outputs_fifo: Box<[u32]>,
}

#[derive(Serialize, Deserialize)]
struct SerializedCanonicalOperation {
    inputs_fifo: Box<[u32]>,
    output_count: u32,
    effect_predecessors: Box<[u32]>,
    flippable: bool,
}

impl Serialize for CanonicalBlock {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SerializedCanonicalBlock {
            finalization: self.finalization,
            input_count: self.input_count,
            operations: self
                .operations
                .iter()
                .map(|operation| SerializedCanonicalOperation {
                    inputs_fifo: operation.inputs_fifo.iter().map(|value| value.get()).collect(),
                    output_count: operation.output_count,
                    effect_predecessors: operation
                        .effect_predecessors
                        .iter()
                        .map(|operation| operation.get())
                        .collect(),
                    flippable: operation.flippable,
                })
                .collect(),
            outputs_fifo: self.outputs_fifo.iter().map(|value| value.get()).collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CanonicalBlock {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error as _;

        let serialized = SerializedCanonicalBlock::deserialize(deserializer)?;
        let mut operations = IndexVec::with_capacity(serialized.operations.len());
        for operation in serialized.operations {
            operations.push(CanonicalOperation {
                inputs_fifo: operation
                    .inputs_fifo
                    .iter()
                    .map(|&value| {
                        CanonicalValueId::try_new(value)
                            .ok_or_else(|| D::Error::custom("canonical value ID is out of range"))
                    })
                    .collect::<Result<_, _>>()?,
                effect_predecessors: operation
                    .effect_predecessors
                    .iter()
                    .map(|&operation| {
                        CanonicalOpId::try_new(operation).ok_or_else(|| {
                            D::Error::custom("canonical operation ID is out of range")
                        })
                    })
                    .collect::<Result<_, _>>()?,
                output_count: operation.output_count,
                flippable: operation.flippable,
            });
        }
        let outputs_fifo = serialized
            .outputs_fifo
            .iter()
            .map(|&value| {
                CanonicalValueId::try_new(value)
                    .ok_or_else(|| D::Error::custom("canonical value ID is out of range"))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            finalization: serialized.finalization,
            input_count: serialized.input_count,
            operations,
            outputs_fifo,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CanonicalBlockKey([u8; 32]);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct OpDescriptor {
    flippable: bool,
    inputs: Vec<CanonicalValueId>,
    output_count: u32,
    effect_predecessors: Vec<CanonicalOpId>,
}

struct Candidate {
    source: OpNodeId,
    descriptor: OpDescriptor,
    successor_fingerprint: [u8; 32],
    first_two_inputs_swapped: bool,
}

struct CanonicalState {
    source_ops: IndexVec<OpNodeId, Option<CanonicalOpId>>,
    source_values: IndexVec<ValueNodeId, Option<CanonicalValueId>>,
    canonical_values: IndexVec<CanonicalValueId, ()>,
    operations: IndexVec<CanonicalOpId, CanonicalOperation>,
    witness: IndexVec<CanonicalOpId, CanonicalOpWitness>,
}

struct Canonicalizer<'a> {
    graph: &'a OpGraph,
    finalization: BlockFinalization,
    input_count: u32,
    effect_predecessors: ListOfLists<OpNodeId, OpNodeId>,
    successor_fingerprints: IndexVec<OpNodeId, [u8; 32]>,
}

pub fn canonicalize_block_for_dedup(block: BlockView<'_>, graph: &OpGraph) -> CanonicalizedBlock {
    canonicalize_graph(graph, BlockFinalization::from_block(block))
}

pub fn deduplication_key(block: BlockView<'_>, graph: &OpGraph) -> CanonicalBlockKey {
    canonicalize_block_for_dedup(block, graph).deduplication_key()
}

impl CanonicalizedBlock {
    pub fn deduplication_key(&self) -> CanonicalBlockKey {
        self.canonical.deduplication_key()
    }

    pub fn block(&self) -> &CanonicalBlock {
        &self.canonical
    }

    pub fn input_count(&self) -> u32 {
        self.canonical.input_count
    }

    pub fn last_op_terminates(&self) -> bool {
        matches!(self.canonical.finalization, BlockFinalization::LastOpTerminates)
    }

    pub fn canonical_op_ids(&self) -> impl Iterator<Item = CanonicalOpId> + '_ {
        self.witness.iter_idx()
    }

    pub fn operation(&self, operation: CanonicalOpId) -> &CanonicalOperation {
        &self.canonical.operations[operation]
    }

    pub fn outputs_fifo(&self) -> &[CanonicalValueId] {
        &self.canonical.outputs_fifo
    }

    pub fn source_op(&self, operation: CanonicalOpId) -> OpNodeId {
        self.witness[operation].source
    }

    pub fn first_two_inputs_swapped(&self, operation: CanonicalOpId) -> bool {
        self.witness[operation].first_two_inputs_swapped
    }
}

impl CanonicalBlockKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for CanonicalBlockKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ssb1:")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl CanonicalBlock {
    pub fn new(
        finalization: BlockFinalization,
        input_count: u32,
        operations: Box<[CanonicalOperation]>,
        outputs_fifo: Box<[CanonicalValueId]>,
    ) -> Self {
        Self {
            finalization,
            input_count,
            operations: IndexVec::from_vec(operations.into_vec()),
            outputs_fifo,
        }
    }

    pub fn operation_ids(&self) -> impl Iterator<Item = CanonicalOpId> + '_ {
        self.operations.iter_idx()
    }

    pub fn operation(&self, operation: CanonicalOpId) -> &CanonicalOperation {
        &self.operations[operation]
    }

    pub fn to_op_graph(&self) -> Result<OpGraph, String> {
        let estimated_values = self.operations.iter().try_fold(
            usize::try_from(self.input_count)
                .map_err(|_| "canonical input count does not fit usize")?,
            |total, operation| {
                let outputs = usize::try_from(operation.output_count)
                    .map_err(|_| "canonical output count does not fit usize")?;
                total.checked_add(outputs).ok_or("canonical value count overflow")
            },
        )?;
        let mut operation_ids = IndexVec::<CanonicalOpId, OpNodeId>::new();
        let mut value_ids = IndexVec::<CanonicalValueId, ValueNodeId>::new();
        let mut graph = OpGraphBuilder::with_capacity(self.operations.len(), estimated_values);
        for _ in 0..self.input_count {
            value_ids.push(graph.push_input_value());
        }
        let mut graph = graph.end_inputs_begin_ops();

        for (canonical_id, operation) in self.operations.enumerate_idx() {
            let source = OperationIdx::try_from(canonical_id.idx())
                .map_err(|_| "canonical operation ID does not fit OperationIdx")?;
            let kind = if operation.flippable {
                OpNodeKind::Flippable(source)
            } else {
                OpNodeKind::Normal(source)
            };
            let mut builder = graph.begin_op(kind);
            for &input in &operation.inputs_fifo {
                let input = value_ids
                    .get(input)
                    .copied()
                    .ok_or_else(|| format!("graph refers to missing v{input}"))?;
                builder.add_input(input);
            }
            for &predecessor in &operation.effect_predecessors {
                let predecessor = operation_ids.get(predecessor).copied().ok_or_else(|| {
                    format!("operation refers to missing predecessor op{predecessor}")
                })?;
                builder.add_predecessor(predecessor);
            }
            let operation_id = builder.id();
            let mut builder = builder.end_inputs_begin_outputs();
            for _ in 0..operation.output_count {
                value_ids.push(builder.add_output());
            }
            assert_eq!(operation_ids.push(operation_id), canonical_id);
        }

        let mut graph = graph.end_ops_begin_end_stack();
        for &output in &self.outputs_fifo {
            let output = value_ids
                .get(output)
                .copied()
                .ok_or_else(|| format!("graph refers to missing v{output}"))?;
            graph.push_end_stack_value(output);
        }
        Ok(graph.finish())
    }

    pub fn deduplication_key(&self) -> CanonicalBlockKey {
        const DOMAIN: &[u8] = b"plank.stack-scheduling.block-key";
        const FORMAT_VERSION: u8 = 1;

        let mut hash = Sha256::new();
        hash.update(DOMAIN);
        hash.update([FORMAT_VERSION]);
        hash.update([match self.finalization {
            BlockFinalization::ShuffleToOutputs => 0,
            BlockFinalization::LastOpTerminates => 1,
        }]);
        update_u32(&mut hash, self.input_count);
        update_len(&mut hash, self.operations.len());

        for operation in self.operations.iter() {
            hash.update([u8::from(operation.flippable)]);
            update_len(&mut hash, operation.inputs_fifo.len());
            for &input in &operation.inputs_fifo {
                update_u32(&mut hash, input.get());
            }
            update_u32(&mut hash, operation.output_count);

            update_len(&mut hash, operation.effect_predecessors.len());
            for &predecessor in &operation.effect_predecessors {
                update_u32(&mut hash, predecessor.get());
            }
        }

        update_len(&mut hash, self.outputs_fifo.len());
        for &output in &self.outputs_fifo {
            update_u32(&mut hash, output.get());
        }

        CanonicalBlockKey(hash.finalize().into())
    }
}

impl Canonicalizer<'_> {
    fn new(graph: &OpGraph, finalization: BlockFinalization) -> Canonicalizer<'_> {
        let mut effect_predecessors =
            ListOfLists::with_capacities(graph.total_ops() as usize, graph.total_ops() as usize);
        for operation in graph.op_ids() {
            let view = graph.get_op(operation);
            let mut data_producers =
                DenseIndexSet::with_capacity_in_bits(graph.total_ops() as usize);
            for &input in view.inputs_fifo {
                if let Some(producer) = graph.get_producer(input) {
                    data_producers.add(producer);
                }
            }
            let stored = effect_predecessors.push_iter(
                graph
                    .get_immediate_predecessors(operation)
                    .filter(|&predecessor| !data_producers.contains(predecessor)),
            );
            assert_eq!(stored, operation);
        }
        let successor_fingerprints = successor_fingerprints(graph, &effect_predecessors);

        Canonicalizer {
            graph,
            finalization,
            input_count: graph.input_values_fifo().len(),
            effect_predecessors,
            successor_fingerprints,
        }
    }

    fn run(&self) -> CanonicalizedBlock {
        let mut source_values = IndexVec::from_vec(vec![None; self.graph.total_values() as usize]);
        let mut canonical_values = IndexVec::with_capacity(self.graph.total_values() as usize);
        for source_input in self.graph.input_values_fifo() {
            let canonical_input = canonical_values.push(());
            assert!(source_values[source_input].replace(canonical_input).is_none());
        }

        let mut state = CanonicalState {
            source_ops: IndexVec::from_vec(vec![None; self.graph.total_ops() as usize]),
            source_values,
            canonical_values,
            operations: IndexVec::with_capacity(self.graph.total_ops() as usize),
            witness: IndexVec::with_capacity(self.graph.total_ops() as usize),
        };

        while state.operations.len() < self.graph.total_ops() as usize {
            let candidate = self
                .graph
                .op_ids()
                .filter(|&source| {
                    state.source_ops[source].is_none()
                        && self
                            .graph
                            .get_predecessors(source)
                            .iter()
                            .all(|predecessor| state.source_ops[predecessor].is_some())
                })
                .map(|source| self.describe_candidate(&state, source))
                .min_by(|left, right| {
                    left.descriptor
                        .cmp(&right.descriptor)
                        .then_with(|| left.successor_fingerprint.cmp(&right.successor_fingerprint))
                        .then_with(|| left.source.cmp(&right.source))
                })
                .expect("operation graph contains a cycle");
            self.assign(&mut state, candidate);
        }

        let outputs_fifo = self
            .graph
            .output_values_fifo()
            .iter()
            .map(|&source| state.source_values[source].expect("output value was not assigned"))
            .collect();
        CanonicalizedBlock {
            canonical: CanonicalBlock {
                finalization: self.finalization,
                input_count: self.input_count,
                operations: state.operations,
                outputs_fifo,
            },
            witness: state.witness,
        }
    }

    fn describe_candidate(&self, state: &CanonicalState, source: OpNodeId) -> Candidate {
        let view = self.graph.get_op(source);
        let flippable = matches!(view.kind, OpNodeKind::Flippable(_));
        let mut inputs = view
            .inputs_fifo
            .iter()
            .map(|&input| state.source_values[input].expect("input producer was not assigned"))
            .collect::<Vec<_>>();
        let first_two_inputs_swapped = if flippable {
            assert!(inputs.len() >= 2, "flippable operation has fewer than two inputs");
            if inputs[0] > inputs[1] {
                inputs.swap(0, 1);
                true
            } else {
                false
            }
        } else {
            false
        };
        let mut effect_predecessors = self.effect_predecessors[source]
            .iter()
            .map(|&predecessor| {
                state.source_ops[predecessor].expect("effect predecessor was not assigned")
            })
            .collect::<Vec<_>>();
        effect_predecessors.sort_unstable();

        Candidate {
            source,
            descriptor: OpDescriptor {
                flippable,
                inputs,
                output_count: view.outputs_fifo.len().try_into().expect("output count overflow"),
                effect_predecessors,
            },
            successor_fingerprint: self.successor_fingerprints[source],
            first_two_inputs_swapped,
        }
    }

    fn assign(&self, state: &mut CanonicalState, candidate: Candidate) {
        let canonical = state.operations.push(CanonicalOperation {
            inputs_fifo: candidate.descriptor.inputs.into_boxed_slice(),
            effect_predecessors: candidate.descriptor.effect_predecessors.into_boxed_slice(),
            output_count: candidate.descriptor.output_count,
            flippable: candidate.descriptor.flippable,
        });
        assert!(state.source_ops[candidate.source].replace(canonical).is_none());
        assert_eq!(
            state.witness.push(CanonicalOpWitness {
                source: candidate.source,
                first_two_inputs_swapped: candidate.first_two_inputs_swapped,
            }),
            canonical
        );

        for &source_output in self.graph.get_op(candidate.source).outputs_fifo {
            let canonical_output = state.canonical_values.push(());
            assert!(state.source_values[source_output].replace(canonical_output).is_none());
        }
    }
}

fn successor_fingerprints(
    graph: &OpGraph,
    effect_predecessors: &ListOfLists<OpNodeId, OpNodeId>,
) -> IndexVec<OpNodeId, [u8; 32]> {
    const DOMAIN: &[u8] = b"plank.stack-scheduling.successor-fingerprint";

    let mut fingerprints = IndexVec::from_vec(vec![None; graph.total_ops() as usize]);
    let operations = graph.op_ids().collect::<Vec<_>>();
    for &operation in operations.iter().rev() {
        let view = graph.get_op(operation);
        let mut hash = Sha256::new();
        hash.update(DOMAIN);
        let flippable = matches!(view.kind, OpNodeKind::Flippable(_));
        hash.update([u8::from(flippable)]);
        update_len(&mut hash, view.inputs_fifo.len());
        update_len(&mut hash, view.outputs_fifo.len());

        for &output in view.outputs_fifo {
            let output_positions = graph
                .output_values_fifo()
                .iter()
                .enumerate()
                .filter_map(|(position, &candidate)| {
                    (candidate == output).then_some(
                        u32::try_from(position)
                            .expect("output position exceeds canonical key limit"),
                    )
                })
                .collect::<Vec<_>>();
            update_len(&mut hash, output_positions.len());
            for position in output_positions {
                update_u32(&mut hash, position);
            }

            let mut consumers = Vec::new();
            for consumer in graph.get_consumers(output).iter() {
                let consumer_view = graph.get_op(consumer);
                let consumer_fingerprint =
                    fingerprints[consumer].expect("consumer fingerprint was not assigned");
                let consumer_flippable = matches!(consumer_view.kind, OpNodeKind::Flippable(_));
                for (position, &input) in consumer_view.inputs_fifo.iter().enumerate() {
                    if input == output {
                        let position = u32::try_from(position)
                            .expect("input position exceeds canonical key limit");
                        let operand_role =
                            if consumer_flippable && position < 2 { 0 } else { position + 1 };
                        consumers.push((operand_role, consumer_fingerprint));
                    }
                }
            }
            consumers.sort_unstable();
            update_len(&mut hash, consumers.len());
            for (operand_role, consumer_fingerprint) in consumers {
                update_u32(&mut hash, operand_role);
                hash.update(consumer_fingerprint);
            }
        }

        let mut effect_successors = graph
            .op_ids()
            .filter(|&successor| effect_predecessors[successor].contains(&operation))
            .map(|successor| {
                fingerprints[successor].expect("effect successor fingerprint was not assigned")
            })
            .collect::<Vec<_>>();
        effect_successors.sort_unstable();
        update_len(&mut hash, effect_successors.len());
        for successor in effect_successors {
            hash.update(successor);
        }

        assert!(fingerprints[operation].replace(hash.finalize().into()).is_none());
    }

    fingerprints
        .iter()
        .map(|fingerprint| fingerprint.expect("operation fingerprint was not assigned"))
        .collect()
}

pub fn canonicalize_graph(graph: &OpGraph, finalization: BlockFinalization) -> CanonicalizedBlock {
    Canonicalizer::new(graph, finalization).run()
}

fn update_len(hash: &mut Sha256, length: usize) {
    update_u32(hash, length.try_into().expect("canonical key list length overflow"));
}

fn update_u32(hash: &mut Sha256, value: u32) {
    hash.update(value.to_le_bytes());
}

#[cfg(test)]
#[path = "canonical_tests.rs"]
mod tests;
