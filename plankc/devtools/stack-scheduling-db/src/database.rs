use crate::model::{representative_graph, representative_schedule};
use plank_core::Idx;
use sir_data::BasicBlockId;
use sir_stack_scheduling::{
    op_graph::{CanonicalizedBlock, OpGraph},
    stack::StackOps,
};
use sir_stack_scheduling_common::{
    BLOCKS_FILE_NAME, BLOCKS_HEADER, BlockRow, CANONICAL_BLOCKS_FILE_NAME, CanonicalBlockRow,
    seed_canonical_database,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

pub struct DatabaseWriter {
    output_directory: PathBuf,
    blocks: csv::Writer<NamedTempFile>,
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
        let file = NamedTempFile::new_in(&output_directory).unwrap();
        let mut blocks = csv::WriterBuilder::new().has_headers(false).from_writer(file);
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
            representative_schedule(source_schedule, graph, canonicalized);
        let gas_cost = representative_schedule.gas_cost();
        let representative_schedule = serde_json::to_string(&representative_schedule)
            .expect("representative schedule serialization failed");

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
        let rows = self
            .canonical_blocks
            .into_iter()
            .map(|(canonical_hash, record)| CanonicalBlockRow {
                canonical_hash,
                canonical_graph: record.graph,
                best_schedule: record.best_schedule,
                best_gas_cost: record.best_gas_cost,
            })
            .collect::<Box<[_]>>();
        seed_canonical_database(&canonical_blocks_path, &rows).unwrap();
        let blocks = self.blocks.into_inner().unwrap();
        blocks.as_file().sync_all().unwrap();
        blocks.persist(self.output_directory.join(BLOCKS_FILE_NAME)).unwrap();

        eprintln!("wrote {}", self.output_directory.join(BLOCKS_FILE_NAME).display());
        eprintln!("wrote {}", canonical_blocks_path.display());
    }
}
