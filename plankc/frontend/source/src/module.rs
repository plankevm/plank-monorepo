use crate::{FILE_EXTENSION, project::ImportKind};
use hashbrown::HashMap;
use plank_parser::{
    StrId,
    ast::{Import, ImportSuffix},
};
use plank_session::Session;
use std::path::PathBuf;

#[derive(Default)]
pub struct ModuleResolver {
    modules: HashMap<StrId, PathBuf>,
}

#[derive(Debug)]
pub enum ModuleResolveError {
    UnknownModule(StrId),
    NotEnoughSegments,
}

#[derive(Debug)]
pub struct ModuleRegisterError;

impl ModuleResolver {
    pub fn register(&mut self, name: StrId, root: PathBuf) -> Result<(), ModuleRegisterError> {
        match self.modules.insert(name, root) {
            Some(_) => Err(ModuleRegisterError),
            None => Ok(()),
        }
    }

    pub fn resolve(
        &self,
        segments: &[StrId],
        import: Import<'_>,
        session: &Session,
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
                ImportKind::Specific {
                    selected_name: last,
                    imported_as: alias.unwrap_or(last),
                    name_span: import.last_path_segment_span(),
                }
            }
            ImportSuffix::All => ImportKind::All,
        };

        import_file_path.clone_from(module_root);
        for &seg in import_path_segments {
            import_file_path.push(session.lookup_name(seg));
        }
        import_file_path.set_extension(FILE_EXTENSION);

        Ok(kind)
    }
}
