use crate::{
    StrId,
    ast::{ImportSuffix, TopLevelDef},
    cst::ConcreteSyntaxTree,
    diagnostics::DiagnosticsContext,
    interner::PlankInterner,
    lexer::Lexed,
    module::{ImportTarget, ModuleResolver},
    parser::parse,
    source::{SourceId, SourceManager},
};
use hashbrown::HashMap;
use sensei_core::{IndexVec, list_of_lists::ListOfLists, newtype_index};
use std::path::{Path, PathBuf};

newtype_index! {
    pub struct ImportIdx;
}

#[derive(Debug, Clone, Copy)]
pub enum ImportKind {
    Specific { selected_name: StrId, imported_as: StrId },
    All,
}

#[derive(Debug, Clone, Copy)]
pub struct FileImport {
    pub kind: ImportKind,
    pub target_source: SourceId,
}

#[derive(Debug)]
pub struct Source {
    pub path: PathBuf,
    pub content: String,
    pub cst: ConcreteSyntaxTree,
}

pub struct ParsedProject {
    pub sources: IndexVec<SourceId, Source>,
    pub imports: ListOfLists<SourceId, FileImport>,
}

struct ProjectParser<'a, D: DiagnosticsContext> {
    interner: &'a mut PlankInterner,
    diagnostics: &'a mut D,

    module_resolver: &'a ModuleResolver,

    // Need `cst` borrowed for the duration of the loop, so we use `None` until we can set it.
    sources: IndexVec<SourceId, (PathBuf, String, Option<ConcreteSyntaxTree>)>,
    file_imports: ListOfLists<SourceId, Option<FileImport>>,
    path_to_source: HashMap<PathBuf, SourceId>,

    segment_buf: Vec<StrId>,
    import_resolved_path: PathBuf,
}

impl<D: DiagnosticsContext> ProjectParser<'_, D> {
    fn parse_source(&mut self, path: PathBuf) -> SourceId {
        let content = std::fs::read_to_string(&path).expect("failed to read source file");
        let source_id = self.sources.next_idx();
        let cst = parse(&Lexed::lex(&content), self.interner, self.diagnostics, source_id);
        self.path_to_source.insert(path.clone(), source_id);

        assert_eq!(self.sources.push((path, content, None)), source_id);
        let file = cst.as_file();

        {
            let alt_source_id = self.file_imports.push_with(|mut imports| {
                for def in file.iter_defs() {
                    if let TopLevelDef::Import(_) = def {
                        imports.push(None);
                    }
                }
            });
            assert_eq!(source_id, alt_source_id);
        }

        let imports = file.iter_defs().filter_map(|def| match def {
            TopLevelDef::Import(import) => Some(import),
            _ => None,
        });
        for (i, import) in imports.enumerate() {
            self.segment_buf.clear();
            import.collect_path_segments(&mut self.segment_buf);
            let ImportTarget(import_target) = self
                .module_resolver
                .resolve(
                    &self.segment_buf,
                    import.is_glob(),
                    self.interner,
                    &mut self.import_resolved_path,
                )
                .expect("todo-diagnostic: failed to resolve import");

            let target_path = self
                .import_resolved_path
                .canonicalize()
                .expect("todo-diagnostic: failed to canonicalize import path");

            let target_source = match self.path_to_source.get(&target_path) {
                Some(&id) => id,
                None => self.parse_source(target_path),
            };
            let kind = match import.suffix {
                None => {
                    let target = import_target.expect("not glob but no target");
                    ImportKind::Specific { selected_name: target, imported_as: target }
                }
                Some(ImportSuffix::As(imported_as)) => {
                    let target = import_target.expect("not glob but no target");
                    ImportKind::Specific { selected_name: target, imported_as }
                }
                Some(ImportSuffix::All) => ImportKind::All,
            };

            let prev = self.file_imports[source_id][i].replace(FileImport { kind, target_source });
            assert!(prev.is_none());
        }

        let prev = self.sources[source_id].2.replace(cst);
        assert!(prev.is_none());

        source_id
    }
}

pub fn parse_project(
    entry_path: &Path,
    module_resolver: &ModuleResolver,
    interner: &mut PlankInterner,
    diagnostics: &mut impl DiagnosticsContext,
) -> ParsedProject {
    let entry_path = entry_path.canonicalize().expect("failed to canonicalize entry path");

    let mut parser = ProjectParser {
        interner,
        diagnostics,
        module_resolver,
        sources: IndexVec::new(),
        file_imports: ListOfLists::new(),
        path_to_source: HashMap::new(),

        segment_buf: Vec::with_capacity(16),
        import_resolved_path: PathBuf::with_capacity(256),
    };

    let entry = parser.parse_source(entry_path);
    assert_eq!(entry, SourceId::ROOT);

    ParsedProject {
        sources: parser
            .sources
            .raw
            .into_iter()
            .map(|(path, content, cst)| Source {
                path,
                content,
                cst: cst.expect("not set in `parse_source`"),
            })
            .collect(),
        imports: {
            let mut imports = ListOfLists::with_capacities(
                parser.file_imports.len(),
                parser.file_imports.total_values(),
            );
            for file_imports in parser.file_imports.iter() {
                imports
                    .push_iter(file_imports.iter().map(|&i| i.expect("not set in `parse_source`")));
            }
            imports
        },
    }
}
