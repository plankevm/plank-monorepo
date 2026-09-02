use clap as _;
use std::path::Path;

mod database;
mod graph;
mod render;

pub use database::{DatabaseEntry, SourceBlock};
pub use graph::Graph;
pub use sir_stack_scheduling_common::{
    BlockFinalization, RepresentativeGraph, RepresentativeOperation, RepresentativeSchedule,
    RepresentativeStackOp,
};

pub fn find(database: &Path, requested_hash: &str) -> Result<DatabaseEntry, String> {
    database::find(database, requested_hash)
}

pub fn random(database: &Path) -> Result<DatabaseEntry, String> {
    database::random(database)
}

pub fn render_source_blocks(source_blocks: &[SourceBlock]) -> String {
    render::source_blocks(source_blocks)
}

pub fn render_graph(graph: &Graph) -> String {
    render::graph(graph)
}

pub fn render_schedule(graph: &Graph, schedule: &RepresentativeSchedule) -> Result<String, String> {
    render::schedule(graph, schedule)
}
