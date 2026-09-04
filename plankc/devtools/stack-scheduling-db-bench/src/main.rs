mod database;
mod graph;
mod stats;

use database::Database;
use graph::{reconstruct, representative_schedule};
use indicatif::{ProgressBar, ProgressStyle};
use sir_stack_scheduling::schedule_graph;
use sir_stack_scheduling_common::{
    CanonicalBlockRow, RepresentativeGraph, RepresentativeSchedule, workspace_corpus_path,
};
use sir_stack_scheduling_db_inspect::{Graph as ValidationGraph, render_graph, trace_schedule};
use stats::Stats;
use std::{process::ExitCode, time::Instant};

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let start = Instant::now();
    let mut database = Database::load(workspace_corpus_path("stack-scheduling-db"))?;
    let graph_count = database.rows.len();
    let progress = ProgressBar::new(
        u64::try_from(graph_count).expect("canonical graph count does not fit u64"),
    );
    progress.set_style(
        ProgressStyle::with_template("{msg} {pos}/{len}").expect("progress template is valid"),
    );
    progress.set_message("processing");
    let mut stats = Stats::new(graph_count);

    for row in &mut database.rows {
        if let Err(error) = process_graph(row, &mut stats) {
            progress.finish_and_clear();
            return Err(error);
        }
        progress.inc(1);
    }
    progress.finish_with_message("processed");

    if stats.has_improvements() {
        database.save()?;
    }
    Ok(stats.render(start.elapsed()))
}

fn process_graph(row: &mut CanonicalBlockRow, stats: &mut Stats) -> Result<(), String> {
    let representative_graph = serde_json::from_str::<RepresentativeGraph>(&row.canonical_graph)
        .map_err(|error| format!("graph {} is invalid: {error}", row.canonical_hash))?;
    let best_known_schedule = serde_json::from_str::<RepresentativeSchedule>(&row.best_schedule)
        .map_err(|error| format!("schedule {} is invalid: {error}", row.canonical_hash))?;
    let encoded_best_known_gas = best_known_schedule.gas_cost();
    if encoded_best_known_gas != row.best_gas_cost {
        return Err(format!(
            "schedule {} has gas cost {encoded_best_known_gas}, database records {}",
            row.canonical_hash, row.best_gas_cost
        ));
    }

    let schedulable = reconstruct(&representative_graph).map_err(|error| {
        format!("graph {} cannot be reconstructed: {error}", row.canonical_hash)
    })?;
    let result = schedule_graph(&schedulable.graph, schedulable.finalization);
    let local_schedule = representative_schedule(&result)
        .map_err(|error| format!("schedule {} cannot be encoded: {error}", row.canonical_hash))?;
    validate_schedule(&row.canonical_hash, representative_graph, &local_schedule)?;

    let local_gas = local_schedule.gas_cost();
    let best_known_gas = row.best_gas_cost;
    stats.record(best_known_gas, local_gas, result.candidate_limit_reached);
    if local_gas < best_known_gas {
        row.best_schedule = serde_json::to_string(&local_schedule).map_err(|error| {
            format!("failed to encode schedule {}: {error}", row.canonical_hash)
        })?;
        row.best_gas_cost = local_gas;
    }
    Ok(())
}

fn validate_schedule(
    canonical_hash: &str,
    representative_graph: RepresentativeGraph,
    schedule: &RepresentativeSchedule,
) -> Result<(), String> {
    let graph = ValidationGraph::from_representative(representative_graph)
        .map_err(|error| format!("graph {canonical_hash} cannot be validated: {error}"))?;
    let trace = trace_schedule(&graph, schedule);
    let Some(error) = trace.error else {
        return Ok(());
    };
    Err(format!(
        "generated invalid schedule for {canonical_hash}\n\ngraph:\n{}\n\nschedule:\n{}\n\nvalidation error: {error}",
        render_graph(&graph),
        trace.rendering
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sir_stack_scheduling_common::{
        BlockFinalization, RepresentativeOperation, RepresentativeStackOp,
    };

    #[test]
    fn replaces_a_worse_best_known_schedule_in_memory() {
        let graph = RepresentativeGraph {
            finalization: BlockFinalization::ShuffleToOutputs,
            input_count: 1,
            operations: Box::new([RepresentativeOperation {
                inputs_fifo: Box::new([0]),
                output_count: 1,
                effect_predecessors: Box::new([]),
                flippable: false,
            }]),
            outputs_fifo: Box::new([1]),
        };
        let baseline = RepresentativeSchedule(Box::new([
            RepresentativeStackOp::Dup { depth: 0 },
            RepresentativeStackOp::Pop,
            RepresentativeStackOp::Op { operation: 0 },
        ]));
        let mut row = CanonicalBlockRow {
            canonical_hash: "ssb1:test".to_owned(),
            canonical_graph: serde_json::to_string(&graph).unwrap(),
            best_schedule: serde_json::to_string(&baseline).unwrap(),
            best_gas_cost: baseline.gas_cost(),
        };
        let mut stats = Stats::new(1);

        process_graph(&mut row, &mut stats).unwrap();

        assert_eq!(row.best_gas_cost, 0);
        assert_eq!(
            serde_json::from_str::<RepresentativeSchedule>(&row.best_schedule).unwrap(),
            RepresentativeSchedule(Box::new([RepresentativeStackOp::Op { operation: 0 }]))
        );
        assert!(stats.has_improvements());
    }
}
