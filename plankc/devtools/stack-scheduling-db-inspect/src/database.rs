use crate::model::{RepresentativeGraph, RepresentativeSchedule};
use rand::Rng;
use serde::Deserialize;
use std::path::{Path, PathBuf};

const BLOCKS_FILE_NAME: &str = "blocks.csv";
const CANONICAL_BLOCKS_FILE_NAME: &str = "canonical-blocks.csv";

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

#[derive(Deserialize)]
struct CanonicalBlockRow {
    canonical_hash: String,
    canonical_graph: String,
    best_schedule: String,
    best_gas_cost: u64,
}

#[derive(Deserialize)]
struct BlockRow {
    file: String,
    block_id: u32,
    canonical_hash: String,
}

pub fn find(database: &Path, requested_hash: &str) -> Result<DatabaseEntry, String> {
    let canonical_blocks_path = canonical_blocks_path(database);
    let normalized_hash = normalize_hash(requested_hash);
    let mut reader = open_database(&canonical_blocks_path)?;

    for row in reader.deserialize::<CanonicalBlockRow>() {
        let row = read_row(row, &canonical_blocks_path)?;
        if row.canonical_hash == normalized_hash {
            return decode(row, database);
        }
    }

    Err(format!("hash '{normalized_hash}' was not found in '{}'", canonical_blocks_path.display()))
}

pub fn random(database: &Path) -> Result<DatabaseEntry, String> {
    let canonical_blocks_path = canonical_blocks_path(database);
    let mut reader = open_database(&canonical_blocks_path)?;
    let mut rng = rand::rng();
    let mut selected = None;

    for (seen, row) in reader.deserialize::<CanonicalBlockRow>().enumerate() {
        let row = read_row(row, &canonical_blocks_path)?;
        if rng.random_range(0..=seen) == 0 {
            selected = Some(row);
        }
    }

    decode(
        selected.ok_or_else(|| {
            format!("'{}' contains no canonical blocks", canonical_blocks_path.display())
        })?,
        database,
    )
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
    if source_blocks.is_empty() {
        return Err(format!("hash '{canonical_hash}' has no entries in '{}'", path.display()));
    }
    Ok(source_blocks.into_boxed_slice())
}

fn blocks_path(database: &Path) -> PathBuf {
    if database.is_dir() {
        database.join(BLOCKS_FILE_NAME)
    } else {
        database.parent().unwrap_or_else(|| Path::new(".")).join(BLOCKS_FILE_NAME)
    }
}

fn canonical_blocks_path(database: &Path) -> PathBuf {
    if database.is_dir() { database.join(CANONICAL_BLOCKS_FILE_NAME) } else { database.to_owned() }
}

fn normalize_hash(hash: &str) -> String {
    if hash.starts_with("ssb1:") { hash.to_owned() } else { format!("ssb1:{hash}") }
}
