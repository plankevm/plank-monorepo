use std::path::{Path, PathBuf};

use crate::{BLOCKS_FILE_NAME, CANONICAL_BLOCKS_FILE_NAME};

pub fn workspace_corpus_path(path: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("stack scheduling common crate is not under the workspace devtools directory")
        .join("corpus")
        .join(path)
}

pub fn blocks_path(database: &Path) -> PathBuf {
    if database.is_dir() {
        database.join(BLOCKS_FILE_NAME)
    } else {
        database.parent().unwrap_or_else(|| Path::new(".")).join(BLOCKS_FILE_NAME)
    }
}

pub fn canonical_blocks_path(database: &Path) -> PathBuf {
    if database.is_dir() { database.join(CANONICAL_BLOCKS_FILE_NAME) } else { database.to_owned() }
}

pub fn normalize_hash(hash: &str) -> String {
    if hash.starts_with("ssb1:") { hash.to_owned() } else { format!("ssb1:{hash}") }
}
