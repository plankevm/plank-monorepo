use sensei_core::{Idx, IndexVec, newtype_index};
use std::path::PathBuf;

newtype_index! {
    pub struct SourceId;
}

impl SourceId {
    pub const ROOT: SourceId = SourceId::ZERO;
}

pub struct SourceInfo {
    pub path: PathBuf,
}

#[derive(Default)]
pub struct SourceManager {
    sources: IndexVec<SourceId, SourceInfo>,
}

impl SourceManager {
    pub fn new(entry_path: PathBuf) -> Self {
        let mut sources = IndexVec::new();
        sources.push(SourceInfo { path: entry_path });
        Self { sources }
    }

    pub fn add_source(&mut self, path: PathBuf) -> SourceId {
        self.sources.push(SourceInfo { path })
    }
}

impl std::ops::Index<SourceId> for SourceManager {
    type Output = SourceInfo;

    fn index(&self, index: SourceId) -> &Self::Output {
        &self.sources[index]
    }
}
