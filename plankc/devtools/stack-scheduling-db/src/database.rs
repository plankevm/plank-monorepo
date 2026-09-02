use crate::model::{representative_graph, representative_schedule, schedule_gas_cost};
use plank_core::Idx;
use sir_data::BasicBlockId;
use sir_stack_scheduling::{
    op_graph::{CanonicalizedBlock, OpGraph},
    stack::StackOps,
};
use sir_stack_scheduling_common::{
    BLOCKS_FILE_NAME, BLOCKS_HEADER, BlockRow, CANONICAL_BLOCKS_FILE_NAME, CANONICAL_BLOCKS_HEADER,
    CanonicalBlockRow,
};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
};

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
        let mut blocks =
            csv::WriterBuilder::new().has_headers(false).from_path(&blocks_path).unwrap_or_else(
                |error| panic!("failed to create '{}': {error}", blocks_path.display()),
            );
        blocks.write_record(BLOCKS_HEADER).unwrap();
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
            .serialize(BlockRow {
                file: file.display().to_string(),
                block_id: block_id.get(),
                canonical_hash: canonical_hash.clone(),
            })
            .unwrap();

        let representative_graph = serde_json::to_string(&representative_graph(canonicalized))
            .expect("representative graph serialization failed");
        let representative_schedule =
            serde_json::to_string(&representative_schedule(source_schedule, graph, canonicalized))
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
        let mut canonical_blocks = csv::WriterBuilder::new()
            .has_headers(false)
            .from_path(&canonical_blocks_path)
            .unwrap_or_else(|error| {
                panic!("failed to create '{}': {error}", canonical_blocks_path.display())
            });
        canonical_blocks.write_record(CANONICAL_BLOCKS_HEADER).unwrap();
        for (canonical_hash, record) in self.canonical_blocks {
            canonical_blocks
                .serialize(CanonicalBlockRow {
                    canonical_hash,
                    canonical_graph: record.graph,
                    best_schedule: record.best_schedule,
                    best_gas_cost: record.best_gas_cost,
                })
                .unwrap();
        }
        canonical_blocks.flush().unwrap();

        eprintln!("wrote {}", self.output_directory.join(BLOCKS_FILE_NAME).display());
        eprintln!("wrote {}", canonical_blocks_path.display());
    }
}
