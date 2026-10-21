use crate::{
    database::{CanonicalBlockRow, CanonicalDatabase, improve_schedule},
    stats::Stats,
};
use indicatif::{ProgressBar, ProgressStyle};
use plank_core::Idx;
use sir_data::StaticAllocId;
use sir_stack_scheduling::{
    BlockFinalization,
    display::{graph as render_graph, trace},
    op_graph::{CanonicalBlock, OpGraph},
    schedule_graph, schedule_graph_with_effort,
    stack::{ShuffleConfig, StackOps, gas_cost},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};

pub fn run(database: &Path, effort: Option<u32>, save: bool) -> Result<String, String> {
    let start = Instant::now();
    let rows = CanonicalDatabase::open(database)?.all()?;
    if rows.is_empty() {
        return Err("database contains no canonical blocks".to_owned());
    }
    let manually_optimized_count = rows.iter().filter(|row| row.manually_optimized).count();
    let progress = ProgressBar::new(
        u64::try_from(rows.len()).expect("canonical graph count does not fit u64"),
    );
    progress.set_style(
        ProgressStyle::with_template("{msg} {pos}/{len}").expect("progress template is valid"),
    );
    progress.set_message("processing");
    let mut stats = Stats::new(rows.len(), manually_optimized_count);
    let mut scheduler_time = Duration::ZERO;

    for row in &rows {
        let schedule = match process_graph(row, effort, &mut stats, &mut scheduler_time) {
            Ok(schedule) => schedule,
            Err(error) => {
                progress.finish_and_clear();
                return Err(error);
            }
        };
        if save && let Some(schedule) = schedule {
            improve_schedule(database, &row.canonical_hash, &schedule)?;
        }
        progress.inc(1);
    }
    progress.finish_and_clear();

    Ok(stats.render(start.elapsed(), scheduler_time))
}

/// Returns the generated schedule when it is cheaper than the best known one.
fn process_graph(
    row: &CanonicalBlockRow,
    effort: Option<u32>,
    stats: &mut Stats,
    scheduler_time: &mut Duration,
) -> Result<Option<Box<[StackOps]>>, String> {
    let canonical = serde_json::from_str::<CanonicalBlock>(&row.canonical_graph)
        .map_err(|error| format!("graph {} is invalid: {error}", row.canonical_hash))?;
    let best_known_schedule = serde_json::from_str::<Box<[StackOps]>>(&row.best_schedule)
        .map_err(|error| format!("schedule {} is invalid: {error}", row.canonical_hash))?;
    let encoded_best_known_gas = gas_cost(&best_known_schedule, ShuffleConfig::PRE_AMSTERDAM);
    if encoded_best_known_gas != row.best_gas_cost {
        return Err(format!(
            "schedule {} has gas cost {encoded_best_known_gas}, database records {}",
            row.canonical_hash, row.best_gas_cost
        ));
    }

    let finalization = canonical.finalization;
    let graph = canonical.to_op_graph().map_err(|error| {
        format!("graph {} cannot be reconstructed: {error}", row.canonical_hash)
    })?;
    let schedule_start = Instant::now();
    let result = match effort {
        Some(effort) => schedule_graph_with_effort(
            &graph,
            finalization,
            StaticAllocId::ZERO,
            ShuffleConfig::PRE_AMSTERDAM,
            effort,
        ),
        None => {
            schedule_graph(&graph, finalization, StaticAllocId::ZERO, ShuffleConfig::PRE_AMSTERDAM)
        }
    };
    *scheduler_time += schedule_start.elapsed();
    validate_schedule(&row.canonical_hash, &graph, finalization, &result.ops)?;

    let local_gas = gas_cost(&result.ops, ShuffleConfig::PRE_AMSTERDAM);
    stats.record(row.best_gas_cost, local_gas, row.manually_optimized);
    Ok((local_gas < row.best_gas_cost).then_some(result.ops))
}

fn validate_schedule(
    canonical_hash: &str,
    graph: &OpGraph,
    finalization: BlockFinalization,
    schedule: &[StackOps],
) -> Result<(), String> {
    let trace = trace(
        graph,
        finalization,
        ShuffleConfig::PRE_AMSTERDAM,
        StaticAllocId::default(),
        schedule,
    );
    let Some(error) = trace.error else {
        return Ok(());
    };
    Err(format!(
        "generated invalid schedule for {canonical_hash}\n\ngraph:\n{}\n\nschedule:\n{}\n\nvalidation error: {error}",
        render_graph(graph),
        trace.rendering
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::create_test_database;
    use sir_data::OperationIdx;
    use sir_stack_scheduling::op_graph::{CanonicalOperation, CanonicalValueId};

    fn worse_than_optimal_row() -> CanonicalBlockRow {
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
        CanonicalBlockRow {
            canonical_hash: "ssb1:test".to_owned(),
            canonical_graph: serde_json::to_string(&graph).unwrap(),
            best_schedule: serde_json::to_string(&baseline).unwrap(),
            best_gas_cost: gas_cost(&baseline, ShuffleConfig::PRE_AMSTERDAM),
            manually_optimized: true,
        }
    }

    #[test]
    fn benchmarks_without_mutating_the_database_unless_saving() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("canonical-blocks.sqlite3");
        create_test_database(&path, &[worse_than_optimal_row()]);
        let before = std::fs::read(&path).unwrap();
        for effort in [None, Some(1), Some(100)] {
            run(&path, effort, false).unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);

        run(&path, None, true).unwrap();
        let rows = CanonicalDatabase::open(&path).unwrap().all().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].best_gas_cost, 0);
        let schedule = serde_json::from_str::<Box<[StackOps]>>(&rows[0].best_schedule).unwrap();
        assert_eq!(schedule.as_ref(), &[StackOps::Op(OperationIdx::ZERO)]);
    }

    #[test]
    fn rejects_an_empty_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("canonical-blocks.sqlite3");
        create_test_database(&path, &[]);
        assert_eq!(run(&path, None, false).unwrap_err(), "database contains no canonical blocks");
    }
}
