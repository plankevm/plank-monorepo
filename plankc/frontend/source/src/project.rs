use crate::{module::ModuleResolver, source_fs::SourceFs};
use hashbrown::HashMap;
use plank_core::{IndexVec, Span, list_of_lists::ListOfLists, newtype_index};
use plank_parser::{
    StrId,
    ast::TopLevelDef,
    cst::ConcreteSyntaxTree,
    lexer::{Lexed, TokenIdx},
    parser::parse,
};
use plank_session::{Session, Source, SourceId};
use std::path::{Path, PathBuf};

newtype_index! {
    pub struct ImportIdx;
}

#[derive(Debug, Clone, Copy)]
pub enum ImportKind {
    Specific { selected_name: StrId, imported_as: StrId, name_span: Span<TokenIdx> },
    All,
}

#[derive(Debug, Clone, Copy)]
pub struct FileImport {
    pub kind: ImportKind,
    pub target_source: SourceId,
    pub span: Span<TokenIdx>,
}

#[derive(Debug)]
pub struct ParsedSource {
    pub lexed: Lexed,
    pub cst: ConcreteSyntaxTree,
}

pub struct ParsedProject {
    pub parsed_sources: IndexVec<SourceId, ParsedSource>,
    pub imports: ListOfLists<SourceId, FileImport>,
}

struct ProjectParser<'a, F: SourceFs> {
    session: &'a mut Session,
    fs: &'a F,

    module_resolver: &'a ModuleResolver,

    parsed_sources: IndexVec<SourceId, (Lexed, Option<ConcreteSyntaxTree>)>,
    file_imports: ListOfLists<SourceId, Option<FileImport>>,
    path_to_source: HashMap<PathBuf, SourceId>,

    segment_buf: Vec<StrId>,
    import_resolved_path: PathBuf,
}

impl<F: SourceFs> ProjectParser<'_, F> {
    fn parse_source(&mut self, path: PathBuf) -> SourceId {
        let content = self.fs.read_to_string(&path).expect("failed to read source file");
        let source_id = self.session.next_source();
        let lexed = Lexed::lex(&content);
        let cst = parse(self.session, &lexed, &content, source_id);
        let prev = self.path_to_source.insert(path.clone(), source_id);
        assert!(prev.is_none());
        assert_eq!(self.session.register_source(Source { path, content }), source_id);

        assert_eq!(self.parsed_sources.push((lexed, None)), source_id);
        let file = cst.as_file();

        assert_eq!(
            source_id,
            self.file_imports.push_with(|mut imports| {
                for def in file.iter_defs() {
                    if let TopLevelDef::Import(_) = def {
                        imports.push(None);
                    }
                }
            })
        );

        let imports = file.iter_defs().filter_map(|def| match def {
            TopLevelDef::Import(import) => Some(import),
            _ => None,
        });
        for (i, import) in imports.enumerate() {
            self.segment_buf.clear();
            import.collect_path_segments(&mut self.segment_buf);
            let import_kind = self
                .module_resolver
                .resolve(&self.segment_buf, import, self.session, &mut self.import_resolved_path)
                .expect("todo-diagnostic: failed to resolve import");

            let target_path = self
                .fs
                .canonicalize(&self.import_resolved_path)
                .expect("todo-diagnostic: failed to canonicalize import path");

            let target_source = match self.path_to_source.get(&target_path) {
                Some(&id) => id,
                None => self.parse_source(target_path),
            };

            let prev = self.file_imports[source_id][i].replace(FileImport {
                kind: import_kind,
                target_source,
                span: import.node().span(),
            });
            assert!(prev.is_none());
        }

        assert!(self.parsed_sources[source_id].1.replace(cst).is_none());

        source_id
    }
}

pub fn parse_project(
    entry_path: &Path,
    module_resolver: &ModuleResolver,
    session: &mut Session,
    fs: &impl SourceFs,
) -> ParsedProject {
    let entry_path =
        fs.canonicalize(entry_path).expect("todo-diagnostic: failed to canonicalize entry path");

    let mut parser = ProjectParser {
        session,
        fs,
        module_resolver,
        parsed_sources: IndexVec::new(),
        file_imports: ListOfLists::new(),
        path_to_source: HashMap::new(),

        segment_buf: Vec::with_capacity(16),
        import_resolved_path: PathBuf::with_capacity(256),
    };

    assert_eq!(parser.parse_source(entry_path), SourceId::ROOT);

    ParsedProject {
        parsed_sources: parser
            .parsed_sources
            .raw
            .into_iter()
            .map(|(lexed, cst)| ParsedSource {
                lexed,
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
