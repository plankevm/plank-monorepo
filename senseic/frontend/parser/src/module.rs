use crate::{FILE_EXTENSION, StrId, interner::PlankInterner};
use hashbrown::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct ModuleResolver {
    modules: HashMap<StrId, PathBuf>,
}

pub struct ResolvedImport {
    pub const_name: Option<StrId>,
}

#[derive(Debug)]
pub enum ModuleResolveError {
    UnknownModule(StrId),
    NotEnoughSegments,
}

impl ModuleResolver {
    pub fn register(&mut self, name: StrId, root: PathBuf) {
        if self.modules.insert(name, root).is_some() {
            todo!("diagnostic: duplicate module");
        }
    }

    /// Resolves an import path to a file path and optional const name.
    ///
    /// Regular: `[module, file_seg..., const_name]` — min 3 segments
    /// Glob:    `[module, file_seg...]` — min 2 segments
    ///
    /// The resolved file path is written into `path_buf`.
    pub fn resolve(
        &self,
        segments: &[StrId],
        is_glob: bool,
        interner: &PlankInterner,
        path_buf: &mut PathBuf,
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

        let Some((&last_seg, dirs)) = file_segments.split_last() else {
            return Err(ModuleResolveError::NotEnoughSegments);
        };

        path_buf.clone_from(root);
        for &seg in dirs {
            path_buf.push(&interner[seg]);
        }
        path_buf.push(&interner[last_seg]);
        path_buf.set_extension(&FILE_EXTENSION[1..]);

        Ok(ResolvedImport { const_name })
    }
}
