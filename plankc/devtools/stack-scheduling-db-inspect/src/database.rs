use sir_stack_scheduling_common::{
    BlockRow, CanonicalBlockRow, CanonicalDatabase, RepresentativeGraph, RepresentativeSchedule,
    blocks_path,
};
use std::path::Path;

pub struct DatabaseEntry {
    pub canonical_hash: String,
    pub graph: RepresentativeGraph,
    pub schedule: RepresentativeSchedule,
    pub gas_cost: u64,
    pub source_blocks: Box<[SourceBlock]>,
}

pub struct SourceBlock {
    pub file: String,
    pub block_id: u32,
}

pub fn find(database: &Path, requested_hash: &str) -> Result<DatabaseEntry, String> {
    decode(CanonicalDatabase::open(database)?.find(requested_hash)?, database)
}

pub fn random(database: &Path) -> Result<DatabaseEntry, String> {
    decode(CanonicalDatabase::open(database)?.random()?, database)
}

fn open_database(path: &Path) -> Result<csv::Reader<std::fs::File>, String> {
    csv::Reader::from_path(path)
        .map_err(|error| format!("failed to open '{}': {error}", path.display()))
}

fn read_row<T>(row: Result<T, csv::Error>, path: &Path) -> Result<T, String> {
    row.map_err(|error| format!("failed to read '{}': {error}", path.display()))
}

fn decode(row: CanonicalBlockRow, database: &Path) -> Result<DatabaseEntry, String> {
    let graph = serde_json::from_str(&row.canonical_graph)
        .map_err(|error| format!("canonical graph is invalid: {error}"))?;
    let schedule = serde_json::from_str(&row.best_schedule)
        .map_err(|error| format!("best schedule is invalid: {error}"))?;
    let source_blocks = source_blocks(database, &row.canonical_hash)?;
    Ok(DatabaseEntry {
        canonical_hash: row.canonical_hash,
        graph,
        schedule,
        gas_cost: row.best_gas_cost,
        source_blocks,
    })
}

fn source_blocks(database: &Path, canonical_hash: &str) -> Result<Box<[SourceBlock]>, String> {
    let path = blocks_path(database);
    let mut reader = open_database(&path)?;
    let mut source_blocks = Vec::new();
    for row in reader.deserialize::<BlockRow>() {
        let row = read_row(row, &path)?;
        if row.canonical_hash == canonical_hash {
            source_blocks.push(SourceBlock { file: row.file, block_id: row.block_id });
        }
    }
    Ok(source_blocks.into_boxed_slice())
}
