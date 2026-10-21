use crate::{
    database::{CanonicalDatabase, DatabaseEntry, ScheduleUpdate, improve_schedule},
    inspect::trace_schedule,
};
use sir_stack_scheduling::{
    display::graph,
    stack::{ShuffleConfig, gas_cost, parse_stack_ops},
};
use std::{fmt::Write, path::Path};

pub struct SubmissionResult {
    pub accepted: bool,
    pub output: String,
}

pub fn run(database: &Path, hash: &str, source: &str) -> Result<SubmissionResult, String> {
    let entry = DatabaseEntry::try_from(CanonicalDatabase::open(database)?.find(hash)?)?;
    let graph_text = graph(&entry.graph);
    let parsed = parse_stack_ops(source, ShuffleConfig::PRE_AMSTERDAM);
    let gas_cost = gas_cost(&parsed.operations, ShuffleConfig::PRE_AMSTERDAM);
    let trace = trace_schedule(&entry.graph, entry.finalization, &parsed.operations);

    let validation_error = trace.error.as_ref().map(ToString::to_string);
    let (accepted, status_text) = match parsed.error.or(validation_error) {
        Some(reason) => (false, format!("rejected: {reason}")),
        None => match improve_schedule(database, &entry.canonical_hash, &parsed.operations)? {
            ScheduleUpdate::Improved { previous_cost, new_cost } => {
                (true, format!("accepted: improved gas cost from {previous_cost} to {new_cost}"))
            }
            ScheduleUpdate::NotImproved { current_cost } => (
                false,
                format!(
                    "rejected: gas cost {gas_cost} does not improve the current best cost {current_cost}"
                ),
            ),
        },
    };

    let mut output = String::new();
    writeln!(output, "hash: {}", entry.canonical_hash).unwrap();
    writeln!(output, "\ngraph:\n{graph_text}").unwrap();
    writeln!(output, "\nsubmitted schedule (gas: {gas_cost}):\n{}", trace.rendering).unwrap();
    write!(output, "\n{status_text}").unwrap();
    Ok(SubmissionResult { accepted, output })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CanonicalBlockRow, create_test_database};
    use plank_core::Idx;
    use plank_test_utils::dedent_preserve_indent;
    use sir_data::OperationIdx;
    use sir_stack_scheduling::{
        BlockFinalization,
        op_graph::{CanonicalBlock, CanonicalOperation, CanonicalValueId},
        stack::StackOps,
    };
    use tempfile::TempDir;

    const HASH: &str = "ssb1:test";
    const DATABASE: &str = "canonical-blocks.sqlite3";

    fn database() -> TempDir {
        let temporary = tempfile::tempdir().unwrap();
        let graph = CanonicalBlock::new(
            BlockFinalization::ShuffleToOutputs,
            1,
            Box::new([CanonicalOperation {
                inputs_fifo: Box::new([CanonicalValueId::ZERO]),
                output_count: 1,
                effect_predecessors: Box::new([]),
                flippable: false,
            }]),
            Box::new([CanonicalValueId::ZERO + 1]),
        );
        let baseline: Box<[StackOps]> =
            Box::new([StackOps::Dup(0), StackOps::Pop, StackOps::Op(OperationIdx::ZERO)]);

        create_test_database(
            &temporary.path().join(DATABASE),
            &[CanonicalBlockRow {
                canonical_hash: HASH.to_owned(),
                canonical_graph: serde_json::to_string(&graph).unwrap(),
                best_schedule: serde_json::to_string(&baseline).unwrap(),
                best_gas_cost: gas_cost(&baseline, ShuffleConfig::PRE_AMSTERDAM),
                manually_optimized: false,
            }],
        );
        temporary
    }

    #[test]
    fn accepts_and_persists_a_better_schedule() {
        let directory = database();
        let database = directory.path().join(DATABASE);
        let result = run(&database, HASH, "op0").unwrap();
        let expected = dedent_preserve_indent(
            r#"
            hash: ssb1:test

            graph:
            inputs: [v0]
            v1 = op0(v0)
            outputs: [v1]

            submitted schedule (gas: 0):
            ; start:  [v0]
            op0       [v1]

            accepted: improved gas cost from 6 to 0
            "#,
        );
        assert!(result.accepted);
        assert_eq!(result.output, expected);

        let updated = DatabaseEntry::try_from(
            CanonicalDatabase::open(&database).unwrap().find(HASH).unwrap(),
        )
        .unwrap();
        assert_eq!(updated.gas_cost, 0);
        assert_eq!(updated.schedule.as_ref(), &[StackOps::Op(OperationIdx::ZERO)]);
    }

    #[test]
    fn rejects_an_invalid_schedule_and_displays_its_valid_prefix() {
        let directory = database();
        let database = directory.path().join(DATABASE);
        let before = CanonicalDatabase::open(&database).unwrap().all().unwrap();
        let result = run(&database, HASH, "pop op0").unwrap();
        assert_eq!(CanonicalDatabase::open(&database).unwrap().all().unwrap(), before);
        let expected = dedent_preserve_indent(
            r#"
            hash: ssb1:test

            graph:
            inputs: [v0]
            v1 = op0(v0)
            outputs: [v1]

            submitted schedule (gas: 3):
            ; start:  [v0]
            pop         []

            rejected: stack operation 2: op0 underflowed the stack
            "#,
        );
        assert!(!result.accepted);
        assert_eq!(result.output, expected);
    }

    #[test]
    fn rejects_an_incomplete_schedule() {
        let directory = database();
        let database = directory.path().join(DATABASE);
        let result = run(&database, HASH, "").unwrap();
        let expected = dedent_preserve_indent(
            r#"
            hash: ssb1:test

            graph:
            inputs: [v0]
            v1 = op0(v0)
            outputs: [v1]

            submitted schedule (gas: 0):
            ; start:  [v0]

            rejected: schedule does not execute op0
            "#,
        );
        assert!(!result.accepted);
        assert_eq!(result.output, expected);
    }

    #[test]
    fn rejects_a_schedule_that_does_not_improve_gas() {
        let directory = database();
        let database = directory.path().join(DATABASE);
        let result = run(&database, HASH, "dup1 pop op0").unwrap();
        let expected = dedent_preserve_indent(
            r#"
            hash: ssb1:test

            graph:
            inputs: [v0]
            v1 = op0(v0)
            outputs: [v1]

            submitted schedule (gas: 6):
            ; start:      [v0]
            dup1      [v0, v0]
            pop           [v0]
            op0           [v1]

            rejected: gas cost 6 does not improve the current best cost 6
            "#,
        );
        assert!(!result.accepted);
        assert_eq!(result.output, expected);
    }
}
