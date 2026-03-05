use crate::{FILE_EXTENSION, StrId, interner::PlankInterner};
use hashbrown::HashMap;
use std::path::PathBuf;

pub struct ModuleManager {
    modules: HashMap<StrId, PathBuf>,
}

pub struct ResolvedImport {
    pub file_path: PathBuf,
    pub const_name: Option<StrId>,
}

#[derive(Debug)]
pub enum ModuleResolveError {
    UnknownModule(StrId),
    NotEnoughSegments,
}

impl Default for ModuleManager {
    fn default() -> Self {
        Self { modules: HashMap::new() }
    }
}

impl ModuleManager {
    pub fn register(&mut self, name: StrId, root: PathBuf) {
        if self.modules.insert(name, root).is_some() {
            todo!("diagnostic: duplicate module");
        }
    }

    /// Resolves an import path to a file path and optional const name.
    ///
    /// Regular: `[module, file_seg..., const_name]` — min 3 segments
    /// Glob:    `[module, file_seg...]` — min 2 segments
    pub fn resolve(
        &self,
        segments: &[StrId],
        is_glob: bool,
        interner: &PlankInterner,
    ) -> Result<ResolvedImport, ModuleResolveError> {
        let Some((&module_name, remaining)) = segments.split_first() else {
            return Err(ModuleResolveError::NotEnoughSegments);
        };
        let root =
            self.modules.get(&module_name).ok_or(ModuleResolveError::UnknownModule(module_name))?;

        let (file_segments, const_name) = if is_glob {
            (remaining, None)
        } else {
            let Some((&last, rest)) = remaining.split_last() else {
                return Err(ModuleResolveError::NotEnoughSegments);
            };
            (rest, Some(last))
        };

        let mut path = root.clone();
        let Some((&last_seg, dirs)) = file_segments.split_last() else {
            return Err(ModuleResolveError::NotEnoughSegments);
        };
        for &seg in dirs {
            path.push(&interner[seg]);
        }
        path.push(format!("{}{FILE_EXTENSION}", &interner[last_seg]));

        Ok(ResolvedImport { file_path: path, const_name })
    }
}
