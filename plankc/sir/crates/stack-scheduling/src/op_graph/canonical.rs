use std::fmt;

use plank_core::{DenseIndexSet, Idx, IndexVec, Span, list_of_lists::ListOfLists, newtype_index};
use sha2::{Digest, Sha256};
use sir_data::{BlockView, ControlView};

use super::{OpGraph, OpNodeId, OpNodeKind, ValueNodeId};

newtype_index! {
    pub struct CanonicalOpId;
    struct CanonicalValueId;
    struct CanonicalInputArenaIdx;
    struct CanonicalPredArenaIdx;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum BlockFinalization {
    ShuffleToOutputs,
    LastOpTerminates,
}

#[derive(Debug, Clone, Copy)]
struct CanonicalOp {
    inputs: Span<CanonicalInputArenaIdx>,
    effect_predecessors: Span<CanonicalPredArenaIdx>,
    output_count: u32,
    flippable: bool,
}

#[derive(Debug)]
struct CanonicalBlock {
    finalization: BlockFinalization,
    input_count: u32,
    operations: IndexVec<CanonicalOpId, CanonicalOp>,
    inputs_arena: IndexVec<CanonicalInputArenaIdx, CanonicalValueId>,
    effect_predecessors_arena: IndexVec<CanonicalPredArenaIdx, CanonicalOpId>,
    outputs_fifo: Box<[CanonicalValueId]>,
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
    operations: IndexVec<CanonicalOpId, CanonicalOp>,
    inputs_arena: IndexVec<CanonicalInputArenaIdx, CanonicalValueId>,
    effect_predecessors_arena: IndexVec<CanonicalPredArenaIdx, CanonicalOpId>,
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
    let finalization = if matches!(block.control(), ControlView::LastOpTerminates) {
        BlockFinalization::LastOpTerminates
    } else {
        BlockFinalization::ShuffleToOutputs
    };
    canonicalize_graph(graph, finalization)
}

pub fn deduplication_key(block: BlockView<'_>, graph: &OpGraph) -> CanonicalBlockKey {
    canonicalize_block_for_dedup(block, graph).deduplication_key()
}

impl CanonicalizedBlock {
    pub fn deduplication_key(&self) -> CanonicalBlockKey {
        self.canonical.deduplication_key()
    }

    pub fn canonical_op_ids(&self) -> impl Iterator<Item = CanonicalOpId> + '_ {
        self.witness.iter_idx()
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
    fn deduplication_key(&self) -> CanonicalBlockKey {
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
            let inputs = &self.inputs_arena[operation.inputs];
            update_len(&mut hash, inputs.len());
            for &input in inputs {
                update_u32(&mut hash, input.get());
            }
            update_u32(&mut hash, operation.output_count);

            let effect_predecessors =
                &self.effect_predecessors_arena[operation.effect_predecessors];
            update_len(&mut hash, effect_predecessors.len());
            for &predecessor in effect_predecessors {
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
            inputs_arena: IndexVec::new(),
            effect_predecessors_arena: IndexVec::new(),
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
                inputs_arena: state.inputs_arena,
                effect_predecessors_arena: state.effect_predecessors_arena,
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
        let inputs_start = state.inputs_arena.len_idx();
        for input in candidate.descriptor.inputs {
            state.inputs_arena.push(input);
        }
        let inputs = Span::new(inputs_start, state.inputs_arena.len_idx());

        let predecessors_start = state.effect_predecessors_arena.len_idx();
        for predecessor in candidate.descriptor.effect_predecessors {
            state.effect_predecessors_arena.push(predecessor);
        }
        let effect_predecessors =
            Span::new(predecessors_start, state.effect_predecessors_arena.len_idx());

        let canonical = state.operations.push(CanonicalOp {
            inputs,
            effect_predecessors,
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

fn canonicalize_graph(graph: &OpGraph, finalization: BlockFinalization) -> CanonicalizedBlock {
    Canonicalizer::new(graph, finalization).run()
}

fn update_len(hash: &mut Sha256, length: usize) {
    update_u32(hash, length.try_into().expect("canonical key list length overflow"));
}

fn update_u32(hash: &mut Sha256, value: u32) {
    hash.update(value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use hashbrown::HashMap;
    use plank_core::IndexVec;
    use sir_data::OperationIdx;

    use super::*;
    use crate::op_graph::OpGraphBuilder;

    #[derive(Clone, Copy)]
    enum TestOpKind {
        Normal,
        Flippable,
        RetDestPush,
    }

    struct TestOp<'a> {
        name: &'a str,
        kind: TestOpKind,
        inputs: Vec<&'a str>,
        outputs: Vec<&'a str>,
        predecessors: Vec<&'a str>,
    }

    struct TestGraph<'a> {
        inputs: Vec<&'a str>,
        operations: Vec<TestOp<'a>>,
        outputs: Vec<&'a str>,
        finalization: BlockFinalization,
    }

    fn directive<'a>(line: &'a str, name: &str) -> Option<&'a str> {
        line.strip_prefix(name)
            .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
            .map(str::trim)
    }

    fn names(source: &str) -> Vec<&str> {
        source.split_whitespace().collect()
    }

    fn parse_operation(line: &str) -> TestOp<'_> {
        let (head, tail) = line.split_once("->").expect("operation is missing `->`");
        let (name, invocation) =
            head.trim().split_once(' ').expect("operation is missing its kind");
        let open = invocation.find('(').expect("operation is missing `(`");
        let kind = match &invocation[..open] {
            "normal" => TestOpKind::Normal,
            "flip" => TestOpKind::Flippable,
            "ret-dest" => TestOpKind::RetDestPush,
            unknown => panic!("unknown operation kind `{unknown}`"),
        };
        let inputs = invocation[open + 1..].strip_suffix(')').expect("operation is missing `)`");
        let (outputs, predecessors) = tail
            .split_once("; after ")
            .map_or((tail, ""), |(outputs, predecessors)| (outputs, predecessors));

        TestOp {
            name,
            kind,
            inputs: names(inputs),
            outputs: names(outputs),
            predecessors: names(predecessors),
        }
    }

    fn parse_test_graph(source: &str) -> TestGraph<'_> {
        let mut inputs = None;
        let mut operations = Vec::new();
        let mut outputs = None;
        let mut finalization = None;

        for line in source.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if let Some(value) = directive(line, "inputs") {
                assert!(inputs.replace(names(value)).is_none(), "duplicate `inputs` directive");
            } else if let Some(value) = directive(line, "outputs") {
                assert!(outputs.replace(names(value)).is_none(), "duplicate `outputs` directive");
            } else if let Some(value) = directive(line, "final") {
                let value = match value {
                    "shuffle" => BlockFinalization::ShuffleToOutputs,
                    "terminate" => BlockFinalization::LastOpTerminates,
                    unknown => panic!("unknown finalization `{unknown}`"),
                };
                assert!(finalization.replace(value).is_none(), "duplicate `final` directive");
            } else {
                operations.push(parse_operation(line));
            }
        }

        TestGraph {
            inputs: inputs.expect("missing `inputs` directive"),
            operations,
            outputs: outputs.expect("missing `outputs` directive"),
            finalization: finalization.expect("missing `final` directive"),
        }
    }

    fn build_test_graph(source: &str) -> (OpGraph, BlockFinalization) {
        let parsed = parse_test_graph(source);
        let estimated_values = parsed.inputs.len()
            + parsed.operations.iter().map(|operation| operation.outputs.len()).sum::<usize>();
        let mut graph = OpGraphBuilder::with_capacity(parsed.operations.len(), estimated_values);
        let mut values = HashMap::new();
        for input in parsed.inputs {
            let value = graph.push_input_value();
            assert!(values.insert(input, value).is_none(), "duplicate value `{input}`");
        }

        let mut graph = graph.end_inputs_begin_ops();
        let mut source_operations = IndexVec::<OperationIdx, ()>::new();
        let mut operations = HashMap::new();
        for operation in parsed.operations {
            let source_operation = source_operations.push(());
            let kind = match operation.kind {
                TestOpKind::Normal => OpNodeKind::Normal(source_operation),
                TestOpKind::Flippable => OpNodeKind::Flippable(source_operation),
                TestOpKind::RetDestPush => OpNodeKind::RetDestPush(source_operation),
            };
            let mut builder = graph.begin_op(kind);
            for predecessor in operation.predecessors {
                builder.add_predecessor(operations[predecessor]);
            }
            for input in operation.inputs {
                builder.add_input(values[input]);
            }
            let operation_id = builder.id();
            let mut builder = builder.end_inputs_begin_outputs();
            for output in operation.outputs {
                let value = builder.add_output();
                assert!(values.insert(output, value).is_none(), "duplicate value `{output}`");
            }
            assert!(
                operations.insert(operation.name, operation_id).is_none(),
                "duplicate operation `{}`",
                operation.name
            );
        }

        let mut graph = graph.end_ops_begin_end_stack();
        for output in parsed.outputs {
            graph.push_end_stack_value(values[output]);
        }
        (graph.finish(), parsed.finalization)
    }

    fn canonical_key(source: &str) -> CanonicalBlockKey {
        let (graph, finalization) = build_test_graph(source);
        canonicalize_graph(&graph, finalization).deduplication_key()
    }

    fn assert_canonicalizes_equal(left: &str, right: &str) {
        assert_eq!(canonical_key(left), canonical_key(right), "left:\n{left}\nright:\n{right}");
    }

    fn assert_canonicalizes_different(left: &str, right: &str) {
        assert_ne!(canonical_key(left), canonical_key(right), "left:\n{left}\nright:\n{right}");
    }

    #[test]
    fn equal_when_independent_operations_are_reordered() {
        assert_canonicalizes_equal(
            r#"
                inputs left right
                make_left normal(left) -> left_value
                make_right normal(right) -> right_value
                combine normal(left_value right_value) -> result
                outputs result
                final shuffle
            "#,
            r#"
                inputs left right
                make_right normal(right) -> right_value
                make_left normal(left) -> left_value
                combine normal(left_value right_value) -> result
                outputs result
                final shuffle
            "#,
        );
    }

    #[test]
    fn equal_when_tied_operations_with_different_consumers_are_reordered() {
        assert_canonicalizes_equal(
            r#"
                inputs value
                make_single normal(value) -> single
                make_repeated normal(value) -> repeated
                use_single normal(single) -> single_result
                use_repeated normal(repeated repeated) -> repeated_result
                outputs single_result repeated_result
                final shuffle
            "#,
            r#"
                inputs value
                make_repeated normal(value) -> repeated
                make_single normal(value) -> single
                use_single normal(single) -> single_result
                use_repeated normal(repeated repeated) -> repeated_result
                outputs single_result repeated_result
                final shuffle
            "#,
        );
    }

    #[test]
    fn equal_for_normal_and_return_destination_push_with_the_same_arity() {
        assert_canonicalizes_equal(
            r#"
                inputs
                make normal() -> value
                outputs value
                final shuffle
            "#,
            r#"
                inputs
                make ret-dest() -> value
                outputs value
                final shuffle
            "#,
        );
    }

    #[test]
    fn equal_when_flippable_inputs_are_reversed() {
        let first = r#"
            inputs left right
            combine flip(left right) -> result
            outputs result
            final shuffle
        "#;
        let second = r#"
            inputs left right
            combine flip(right left) -> result
            outputs result
            final shuffle
        "#;

        assert_canonicalizes_equal(first, second);

        let (graph, finalization) = build_test_graph(second);
        let canonicalized = canonicalize_graph(&graph, finalization);
        let operation = canonicalized.canonical_op_ids().next().unwrap();
        assert!(canonicalized.first_two_inputs_swapped(operation));
    }

    #[test]
    fn not_equal_when_unflippable_inputs_are_reversed() {
        assert_canonicalizes_different(
            r#"
                inputs left right
                combine normal(left right) -> result
                outputs result
                final shuffle
            "#,
            r#"
                inputs left right
                combine normal(right left) -> result
                outputs result
                final shuffle
            "#,
        );
    }

    #[test]
    fn equal_when_an_ordering_edge_is_transitively_redundant() {
        assert_canonicalizes_equal(
            r#"
                inputs
                first normal() ->
                second normal() -> ; after first
                third normal() -> ; after second
                outputs
                final shuffle
            "#,
            r#"
                inputs
                first normal() ->
                second normal() -> ; after first
                third normal() -> ; after first second
                outputs
                final shuffle
            "#,
        );
    }

    #[test]
    fn not_equal_when_necessary_ordering_differs() {
        assert_canonicalizes_different(
            r#"
                inputs
                first normal() ->
                second normal() ->
                outputs
                final shuffle
            "#,
            r#"
                inputs
                first normal() ->
                second normal() -> ; after first
                outputs
                final shuffle
            "#,
        );
    }

    #[test]
    fn not_equal_when_finalization_differs() {
        assert_canonicalizes_different(
            r#"
                inputs
                outputs
                final shuffle
            "#,
            r#"
                inputs
                outputs
                final terminate
            "#,
        );
    }

    #[test]
    fn key_has_versioned_hex_display() {
        let key = canonical_key(
            r#"
                inputs
                outputs
                final shuffle
            "#,
        );

        assert_eq!(
            key.to_string(),
            "ssb1:105c3a3c4eade43a0d32470e29c3fde6612c883f20b1d7514299b2ba8d2f9d87"
        );
    }
}
