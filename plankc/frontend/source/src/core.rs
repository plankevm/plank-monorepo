use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct CorePaths {
    pub ops: Option<PathBuf>,
    pub interfaces: Option<PathBuf>,
}

impl CorePaths {
    pub fn from_std_root(root: &Path) -> Self {
        Self {
            ops: Some(root.join("core/ops.plk")),
            interfaces: Some(root.join("core/interfaces.plk")),
        }
    }
}
