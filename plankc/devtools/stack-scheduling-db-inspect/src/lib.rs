use clap as _;
use sir_data::StaticAllocId;
use sir_stack_scheduling::{
    BlockFinalization,
    display::{ScheduleTrace, graph, trace},
    op_graph::OpGraph,
    stack::{ShuffleConfig, StackOps},
};
use std::path::Path;

#[cfg(test)]
use tempfile as _;

mod database;
mod render;

pub use database::{DatabaseEntry, SourceBlock};

pub fn find(database: &Path, requested_hash: &str) -> Result<DatabaseEntry, String> {
    database::find(database, requested_hash)
}

pub fn random(database: &Path) -> Result<DatabaseEntry, String> {
    database::random(database)
}

pub fn render_source_blocks(source_blocks: &[SourceBlock]) -> String {
    render::source_blocks(source_blocks)
}

pub fn render_graph(graph_value: &OpGraph) -> String {
    graph(graph_value)
}

pub fn render_schedule(
    graph: &OpGraph,
    finalization: BlockFinalization,
    schedule: &[StackOps],
) -> Result<String, String> {
    let trace = trace_schedule(graph, finalization, schedule);
    match trace.error {
        Some(error) => Err(error.to_string()),
        None => Ok(trace.rendering),
    }
}

pub fn trace_schedule(
    graph: &OpGraph,
    finalization: BlockFinalization,
    schedule: &[StackOps],
) -> ScheduleTrace {
    trace(graph, finalization, ShuffleConfig::PRE_AMSTERDAM, StaticAllocId::default(), schedule)
}
