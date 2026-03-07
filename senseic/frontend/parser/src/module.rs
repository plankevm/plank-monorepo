use crate::{FILE_EXTENSION, StrId, interner::PlankInterner};
use hashbrown::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct ModuleResolver {
    modules: HashMap<StrId, PathBuf>,
}

pub struct ImportTarget(pub Option<StrId>);

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
        import_file_path: &mut PathBuf,
    ) -> Result<ImportTarget, ModuleResolveError> {
        let Some((&module_name, mut import_path_segments)) = segments.split_first() else {
            return Err(ModuleResolveError::NotEnoughSegments);
        };
        let Some(module_root) = self.modules.get(&module_name) else {
            return Err(ModuleResolveError::UnknownModule(module_name));
        };

        let mut import_target_name = None;
        if !is_glob {
            let Some((&last, rest)) = import_path_segments.split_last() else {
                return Err(ModuleResolveError::NotEnoughSegments);
            };
            import_target_name = Some(last);
            import_path_segments = rest;
        }

        import_file_path.clone_from(module_root);
        for &seg in import_path_segments {
            import_file_path.push(&interner[seg]);
        }
        import_file_path.set_extension(FILE_EXTENSION);

        Ok(ImportTarget(import_target_name))
    }
}
