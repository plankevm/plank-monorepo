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
    fmt::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct Options {
    pub effort: Option<u32>,
    pub save: bool,
    pub json: bool,
    pub worst: Option<usize>,
    pub per_graph: Option<PathBuf>,
}

struct GraphResult<'a> {
    hash: &'a str,
    ops: u32,
    inputs: u32,
    outputs: usize,
    gas: u64,
    best_known_gas: u64,
    time: Duration,
}

impl GraphResult<'_> {
    fn gap(&self) -> i64 {
        i64::try_from(self.gas).expect("gas fits i64")
            - i64::try_from(self.best_known_gas).expect("gas fits i64")
    }
}

const PER_GRAPH_HEADER: &str = "hash\tops\tinputs\toutputs\tgas\tbest_known_gas\tgap\tmicros";

pub fn run(database: &Path, options: &Options) -> Result<String, String> {
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
    let mut results = Vec::with_capacity(rows.len());

    for row in &rows {
        let (result, schedule) = match process_graph(row, options.effort) {
            Ok(processed) => processed,
            Err(error) => {
                progress.finish_and_clear();
                return Err(error);
            }
        };
        stats.record(result.best_known_gas, result.gas, row.manually_optimized);
        scheduler_time += result.time;
        if options.save && result.gas < result.best_known_gas {
            improve_schedule(database, &row.canonical_hash, &schedule)?;
        }
        results.push(result);
        progress.inc(1);
    }
    progress.finish_and_clear();
    results.sort_by(|a, b| b.gap().cmp(&a.gap()).then_with(|| a.hash.cmp(b.hash)));

    if let Some(path) = &options.per_graph {
        let table = render_per_graph(&results, results.len());
        std::fs::write(path, table)
            .map_err(|error| format!("failed to write '{}': {error}", path.display()))?;
    }
    let elapsed = start.elapsed();
    let mut output = if options.json {
        render_json(&results, elapsed, scheduler_time)
    } else {
        stats.render(elapsed, scheduler_time)
    };
    if let Some(count) = options.worst {
        write!(output, "\n\nWorst {count} graphs by gap (gas − best known):\n").unwrap();
        output.push_str(render_per_graph(&results, count).trim_end());
    }
    Ok(output)
}

fn render_per_graph(results: &[GraphResult<'_>], count: usize) -> String {
    let mut table = format!("{PER_GRAPH_HEADER}\n");
    for result in results.iter().take(count) {
        writeln!(
            table,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            result.hash,
            result.ops,
            result.inputs,
            result.outputs,
            result.gas,
            result.best_known_gas,
            result.gap(),
            result.time.as_micros()
        )
        .unwrap();
    }
    table
}

fn render_json(results: &[GraphResult<'_>], elapsed: Duration, scheduler_time: Duration) -> String {
    let total_gas: u64 = results.iter().map(|result| result.gas).sum();
    let best_known_gas: u64 = results.iter().map(|result| result.best_known_gas).sum();
    serde_json::json!({
        "graphs": results.len(),
        "total_gas": total_gas,
        "best_known_gas": best_known_gas,
        "graphs_at_best_known": results.iter().filter(|result| result.gap() <= 0).count(),
        "scheduler_seconds": scheduler_time.as_secs_f64(),
        "elapsed_seconds": elapsed.as_secs_f64(),
    })
    .to_string()
}

fn process_graph(
    row: &CanonicalBlockRow,
    effort: Option<u32>,
) -> Result<(GraphResult<'_>, Box<[StackOps]>), String> {
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
    let time = schedule_start.elapsed();
    validate_schedule(&row.canonical_hash, &graph, finalization, &result.ops)?;

    let graph_result = GraphResult {
        hash: &row.canonical_hash,
        ops: graph.total_ops(),
        inputs: graph.input_values_fifo().len(),
        outputs: graph.output_values_fifo().len(),
        gas: gas_cost(&result.ops, ShuffleConfig::PRE_AMSTERDAM),
        best_known_gas: row.best_gas_cost,
        time,
    };
    Ok((graph_result, result.ops))
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
            run(&path, &Options { effort, ..Options::default() }).unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);

        run(&path, &Options { save: true, ..Options::default() }).unwrap();
        let rows = CanonicalDatabase::open(&path).unwrap().all().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].best_gas_cost, 0);
        let schedule = serde_json::from_str::<Box<[StackOps]>>(&rows[0].best_schedule).unwrap();
        assert_eq!(schedule.as_ref(), &[StackOps::Op(OperationIdx::ZERO)]);
    }

    #[test]
    fn per_graph_rows_render_gap_and_time() {
        let results = [
            GraphResult {
                hash: "ssb1:a",
                ops: 3,
                inputs: 2,
                outputs: 1,
                gas: 12,
                best_known_gas: 9,
                time: Duration::from_micros(7),
            },
            GraphResult {
                hash: "ssb1:b",
                ops: 1,
                inputs: 0,
                outputs: 1,
                gas: 3,
                best_known_gas: 3,
                time: Duration::from_micros(1),
            },
        ];
        assert_eq!(
            render_per_graph(&results, 5),
            "hash\tops\tinputs\toutputs\tgas\tbest_known_gas\tgap\tmicros\n\
             ssb1:a\t3\t2\t1\t12\t9\t3\t7\n\
             ssb1:b\t1\t0\t1\t3\t3\t0\t1\n"
        );
        assert_eq!(
            render_per_graph(&results, 1),
            "hash\tops\tinputs\toutputs\tgas\tbest_known_gas\tgap\tmicros\n\
             ssb1:a\t3\t2\t1\t12\t9\t3\t7\n"
        );
    }

    #[test]
    fn rejects_an_empty_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("canonical-blocks.sqlite3");
        create_test_database(&path, &[]);
        assert_eq!(
            run(&path, &Options::default()).unwrap_err(),
            "database contains no canonical blocks"
        );
    }
}
