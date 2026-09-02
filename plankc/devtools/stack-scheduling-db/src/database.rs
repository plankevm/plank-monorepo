use crate::model::{RepresentativeGraph, RepresentativeSchedule, schedule_gas_cost};
use plank_core::Idx;
use sir_data::BasicBlockId;
use sir_stack_scheduling::{
    op_graph::{CanonicalizedBlock, OpGraph},
    stack::StackOps,
};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
};

pub const BLOCKS_FILE_NAME: &str = "blocks.csv";
pub const CANONICAL_BLOCKS_FILE_NAME: &str = "canonical-blocks.csv";

pub struct DatabaseWriter {
    output_directory: PathBuf,
    blocks: csv::Writer<File>,
    canonical_blocks: BTreeMap<String, CanonicalBlockRecord>,
}

struct CanonicalBlockRecord {
    graph: String,
    best_schedule: String,
    best_gas_cost: u64,
}

impl DatabaseWriter {
    pub fn create(output_directory: PathBuf) -> Self {
        fs::create_dir_all(&output_directory).unwrap_or_else(|error| {
            panic!("failed to create '{}': {error}", output_directory.display())
        });
        let blocks_path = output_directory.join(BLOCKS_FILE_NAME);
        let mut blocks = csv::Writer::from_path(&blocks_path).unwrap_or_else(|error| {
            panic!("failed to create '{}': {error}", blocks_path.display())
        });
        blocks.write_record(["file", "block_id", "canonical_hash"]).unwrap();
        Self { output_directory, blocks, canonical_blocks: BTreeMap::new() }
    }

    pub fn collect(
        &mut self,
        file: &Path,
        block_id: BasicBlockId,
        graph: &OpGraph,
        canonicalized: &CanonicalizedBlock,
        source_schedule: &[StackOps],
    ) {
        let canonical_hash = canonicalized.deduplication_key().to_string();
        self.blocks
            .write_record([
                file.display().to_string(),
                block_id.get().to_string(),
                canonical_hash.clone(),
            ])
            .unwrap();

        let representative_graph =
            serde_json::to_string(&RepresentativeGraph::from_canonical(canonicalized))
                .expect("representative graph serialization failed");
        let representative_schedule = serde_json::to_string(&RepresentativeSchedule::from_source(
            source_schedule,
            graph,
            canonicalized,
        ))
        .expect("representative schedule serialization failed");
        let gas_cost = schedule_gas_cost(source_schedule);

        match self.canonical_blocks.entry(canonical_hash) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(CanonicalBlockRecord {
                    graph: representative_graph,
                    best_schedule: representative_schedule,
                    best_gas_cost: gas_cost,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                assert_eq!(
                    entry.get().graph,
                    representative_graph,
                    "equal canonical hashes produced different representative graphs"
                );
                if gas_cost < entry.get().best_gas_cost {
                    entry.insert(CanonicalBlockRecord {
                        graph: representative_graph,
                        best_schedule: representative_schedule,
                        best_gas_cost: gas_cost,
                    });
                }
            }
        }
    }

    pub fn finish(mut self) {
        self.blocks.flush().unwrap();

        let canonical_blocks_path = self.output_directory.join(CANONICAL_BLOCKS_FILE_NAME);
        let mut canonical_blocks =
            csv::Writer::from_path(&canonical_blocks_path).unwrap_or_else(|error| {
                panic!("failed to create '{}': {error}", canonical_blocks_path.display())
            });
        canonical_blocks
            .write_record(["canonical_hash", "canonical_graph", "best_schedule", "best_gas_cost"])
            .unwrap();
        for (canonical_hash, record) in self.canonical_blocks {
            canonical_blocks
                .write_record([
                    canonical_hash,
                    record.graph,
                    record.best_schedule,
                    record.best_gas_cost.to_string(),
                ])
                .unwrap();
        }
        canonical_blocks.flush().unwrap();

        eprintln!("wrote {}", self.output_directory.join(BLOCKS_FILE_NAME).display());
        eprintln!("wrote {}", canonical_blocks_path.display());
    }
}
