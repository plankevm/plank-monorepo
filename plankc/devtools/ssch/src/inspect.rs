use crate::database::{CanonicalDatabase, DatabaseEntry};
use sir_data::StaticAllocId;
use sir_stack_scheduling::{
    BlockFinalization,
    display::{ScheduleTrace, graph, trace},
    op_graph::OpGraph,
    stack::{ShuffleConfig, StackOps},
};
use std::path::Path;

pub fn run(database: &Path, hash: Option<&str>, random: bool) -> Result<String, String> {
    let database = CanonicalDatabase::open(database)?;
    let row = if random {
        database.random()?
    } else {
        database.find(hash.expect("hash is required unless --random is present"))?
    };
    let entry = DatabaseEntry::try_from(row)?;
    let trace = trace_schedule(&entry.graph, entry.finalization, &entry.schedule);
    if let Some(error) = trace.error {
        return Err(error.to_string());
    }
    Ok(format!(
        "hash: {}\n\ngraph:\n{}\n\nbest schedule (gas: {}):\n{}",
        entry.canonical_hash,
        graph(&entry.graph),
        entry.gas_cost,
        trace.rendering
    ))
}

pub fn trace_schedule(
    graph: &OpGraph,
    finalization: BlockFinalization,
    schedule: &[StackOps],
) -> ScheduleTrace {
    trace(graph, finalization, ShuffleConfig::PRE_AMSTERDAM, StaticAllocId::default(), schedule)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::{CanonicalBlockRow, create_test_database};
    use plank_test_utils::dedent_preserve_indent;

    #[test]
    fn renders_a_complete_entry_by_hash_or_random_selection() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("canonical-blocks.sqlite3");
        create_test_database(&path, &[CanonicalBlockRow {
            canonical_hash: "ssb1:test".to_owned(),
            canonical_graph: r#"{"finalization":"shuffle_to_outputs","input_count":1,"operations":[],"outputs_fifo":[0]}"#.to_owned(),
            best_schedule: "[]".to_owned(),
            best_gas_cost: 0,
            manually_optimized: false,
        }]);
        let expected = dedent_preserve_indent(
            r#"
            hash: ssb1:test

            graph:
            inputs: [v0]
            outputs: [v0]

            best schedule (gas: 0):
            ; start:  [v0]
        "#,
        );
        for (hash, random) in
            [(Some("ssb1:test"), false), (Some("test"), false), (Some("tes"), false), (None, true)]
        {
            assert_eq!(run(&path, hash, random).unwrap(), expected);
        }
    }
}
