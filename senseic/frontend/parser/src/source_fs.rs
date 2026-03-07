use hashbrown::HashMap;
use std::{
    io,
    path::{Component, Path, PathBuf},
};

pub trait SourceFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
}

/// Delegates to [`std::fs`].
pub struct RealFs;

impl SourceFs for RealFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }
}

/// In-memory filesystem for testing. Paths are normalized on insertion and lookup
/// (`.` segments removed, `..` segments resolved).
pub struct InMemoryFs {
    files: HashMap<PathBuf, String>,
}

impl Default for InMemoryFs {
    fn default() -> Self {
        Self { files: HashMap::new() }
    }
}

impl InMemoryFs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, path: impl AsRef<Path>, content: String) {
        let path = normalize_path(path.as_ref());
        self.files.insert(path, content);
    }
}

impl SourceFs for InMemoryFs {
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let normalized = normalize_path(path);
        if self.files.contains_key(&normalized) {
            Ok(normalized)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found in InMemoryFs: {}", normalized.display()),
            ))
        }
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let normalized = normalize_path(path);
        self.files.get(&normalized).cloned().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("file not found in InMemoryFs: {}", normalized.display()),
            )
        })
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other),
        }
    }
    result
}
