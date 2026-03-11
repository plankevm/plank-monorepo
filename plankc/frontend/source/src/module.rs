use crate::{FILE_EXTENSION, project::ImportKind};
use hashbrown::HashMap;
use plank_parser::{
    StrId,
    ast::{Import, ImportSuffix},
    interner::PlankInterner,
};
use std::path::PathBuf;

#[derive(Default)]
pub struct ModuleResolver {
    /// Maps between module name and its path.
    modules: HashMap<StrId, PathBuf>,
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
        import: Import<'_>,
        interner: &PlankInterner,
        import_file_path: &mut PathBuf,
    ) -> Result<ImportKind, ModuleResolveError> {
        let Some((&module_name, mut import_path_segments)) = segments.split_first() else {
            return Err(ModuleResolveError::NotEnoughSegments);
        };
        let Some(module_root) = self.modules.get(&module_name) else {
            return Err(ModuleResolveError::UnknownModule(module_name));
        };

        let kind = match import.suffix {
            ImportSuffix::As(alias) => {
                let Some((&last, rest)) = import_path_segments.split_last() else {
                    return Err(ModuleResolveError::NotEnoughSegments);
                };
                import_path_segments = rest;
                ImportKind::Specific { selected_name: last, imported_as: alias.unwrap_or(last) }
            }
            ImportSuffix::All => ImportKind::All,
        };

        import_file_path.clone_from(module_root);
        for &seg in import_path_segments {
            import_file_path.push(&interner[seg]);
        }
        import_file_path.set_extension(FILE_EXTENSION);

        Ok(kind)
    }
}
