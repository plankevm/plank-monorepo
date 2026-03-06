use crate::{
    StrId,
    ast::{File, ImportKind, TopLevelDef},
    cst::ConcreteSyntaxTree,
    diagnostics::DiagnosticsContext,
    interner::PlankInterner,
    lexer::Lexed,
    module::ModuleResolver,
    parser::parse,
    source::{SourceId, SourceManager},
};
use hashbrown::HashMap;
use sensei_core::{IndexVec, list_of_lists::ListOfLists};
use std::path::{Path, PathBuf};

pub struct FileImport {
    pub local_name: Option<StrId>,
    pub target_source: SourceId,
    pub target_const: Option<StrId>,
}

pub struct ParsedProject {
    pub source_manager: SourceManager,
    pub sources: IndexVec<SourceId, String>,
    pub csts: IndexVec<SourceId, ConcreteSyntaxTree>,
    pub imports: ListOfLists<SourceId, FileImport>,
    pub entry: SourceId,
}

struct ProjectParser<'a, D: DiagnosticsContext> {
    source_manager: SourceManager,
    sources: IndexVec<SourceId, String>,
    csts: IndexVec<SourceId, ConcreteSyntaxTree>,
    file_imports: IndexVec<SourceId, Vec<FileImport>>,
    path_to_source: HashMap<PathBuf, SourceId>,
    segment_buf: Vec<StrId>,
    resolve_buf: PathBuf,
    import_buf: Vec<(PathBuf, Option<ImportKind>, Option<StrId>)>,
    module_resolver: &'a ModuleResolver,
    interner: &'a mut PlankInterner,
    diagnostics: &'a mut D,
}

impl<D: DiagnosticsContext> ProjectParser<'_, D> {
    fn parse_source(&mut self, path: PathBuf) -> SourceId {
        let source = std::fs::read_to_string(&path).expect("failed to read source file");
        let source_id = self.sources.next_idx();
        let cst = parse(&Lexed::lex(&source), self.interner, self.diagnostics, source_id);

        self.path_to_source.insert(path.clone(), source_id);

        let import_start = self.import_buf.len();
        for def in File::new(cst.file_view()).expect("failed to init file from CST").iter_defs() {
            let TopLevelDef::Import(import) = def else { continue };

            import.collect_path_segments(&mut self.segment_buf);
            let resolved = self
                .module_resolver
                .resolve(&self.segment_buf, import.is_glob(), self.interner, &mut self.resolve_buf)
                .expect("failed to resolve import");

            let target_path =
                self.resolve_buf.canonicalize().expect("failed to canonicalize import path");

            self.import_buf.push((target_path, import.kind, resolved.const_name));
        }

        assert_eq!(self.source_manager.add_source(path), source_id);
        assert_eq!(self.sources.push(source), source_id);
        assert_eq!(self.csts.push(cst), source_id);
        assert_eq!(self.file_imports.push(Vec::new()), source_id);

        let mut file_imports = Vec::new();
        for i in import_start..self.import_buf.len() {
            let (ref target_path, kind, const_name) = self.import_buf[i];

            let target_source = if let Some(&id) = self.path_to_source.get(target_path) {
                id
            } else {
                self.parse_source(target_path.clone())
            };

            file_imports.push(match kind {
                Some(ImportKind::As(alias)) => {
                    FileImport { local_name: Some(alias), target_source, target_const: const_name }
                }
                Some(ImportKind::All) => {
                    FileImport { local_name: None, target_source, target_const: None }
                }
                None => {
                    let const_name = const_name.expect("non-glob import has const name");
                    FileImport {
                        local_name: Some(const_name),
                        target_source,
                        target_const: Some(const_name),
                    }
                }
            });
        }
        self.import_buf.truncate(import_start);
        self.file_imports[source_id] = file_imports;

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
        source_manager: SourceManager::default(),
        sources: IndexVec::new(),
        csts: IndexVec::new(),
        file_imports: IndexVec::new(),
        path_to_source: HashMap::new(),
        segment_buf: Vec::new(),
        resolve_buf: PathBuf::new(),
        import_buf: Vec::new(),
        module_resolver,
        interner,
        diagnostics,
    };

    let entry = parser.parse_source(entry_path);

    let mut imports = ListOfLists::new();
    for file_imports in parser.file_imports.raw {
        imports.push_iter(file_imports.into_iter());
    }

    ParsedProject {
        source_manager: parser.source_manager,
        sources: parser.sources,
        csts: parser.csts,
        imports,
        entry,
    }
}
