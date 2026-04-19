use crate::{
    diagnostics::{
        error_failed_to_canonicalize_entry, error_failed_to_canonicalize_import,
        error_failed_to_read_source, error_failed_to_resolve_import,
    },
    module::{ModuleResolveError, ModuleResolver},
    source_fs::SourceFs,
};
use hashbrown::HashMap;
use plank_core::{IndexVec, list_of_lists::ListOfLists, newtype_index};
use plank_parser::{
    ast::TopLevelDef,
    cst::ConcreteSyntaxTree,
    lexer::{Lexed, TokenSpan},
    parser::parse,
};
use plank_session::{Session, Source, SourceId, SourceSpan, StrId};
use std::path::{Path, PathBuf};

newtype_index! {
    pub struct ImportIdx;
}

#[derive(Debug, Clone, Copy)]
pub enum ImportKind {
    Specific { selected_name: StrId, imported_as: StrId, name_span: TokenSpan },
    All,
}

#[derive(Debug, Clone, Copy)]
pub struct FileImport {
    pub kind: ImportKind,
    pub target_source: SourceId,
    pub span: TokenSpan,
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
    fn parse_source(&mut self, path: PathBuf) -> Option<SourceId> {
        let content = match self.fs.read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                error_failed_to_read_source(self.session, &path, &err);
                return None;
            }
        };
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
                    match def {
                        TopLevelDef::Import(_) => imports.push(None),
                        TopLevelDef::ImportGroup(group) => {
                            for _ in group.items() {
                                imports.push(None);
                            }
                        }
                        _ => {}
                    }
                }
            })
        );

        let mut import_idx = 0usize;
        for def in file.iter_defs() {
            match def {
                TopLevelDef::Import(import) => {
                    self.segment_buf.clear();
                    import.collect_path_segments(&mut self.segment_buf);
                    let import_kind = match self.module_resolver.resolve_import(
                        &self.segment_buf,
                        import,
                        self.session,
                        &mut self.import_resolved_path,
                    ) {
                        Ok(import_kind) => import_kind,
                        Err(err) => {
                            let span = match &err {
                                ModuleResolveError::UnknownModule(_) => {
                                    import.first_path_segment_span()
                                }
                                ModuleResolveError::NotEnoughSegments => import.node().span(),
                            };
                            let source_span = self.source_span(source_id, span);
                            error_failed_to_resolve_import(
                                self.session,
                                source_id,
                                source_span,
                                &err,
                            );
                            import_idx += 1;
                            continue;
                        }
                    };

                    let target_path =
                        match self.canonicalize_import(source_id, import.file_path_span()) {
                            Some(path) => path,
                            None => {
                                import_idx += 1;
                                continue;
                            }
                        };

                    let target_source = match self.resolve_or_parse_source(target_path) {
                        Some(id) => id,
                        None => {
                            import_idx += 1;
                            continue;
                        }
                    };

                    let prev = self.file_imports[source_id][import_idx].replace(FileImport {
                        kind: import_kind,
                        target_source,
                        span: import.node().span(),
                    });
                    assert!(prev.is_none());
                    import_idx += 1;
                }
                TopLevelDef::ImportGroup(group) => {
                    self.segment_buf.clear();
                    group.collect_path_segments(&mut self.segment_buf);

                    let file_path_resolved = match self.module_resolver.resolve_group_import(
                        &self.segment_buf,
                        self.session,
                        &mut self.import_resolved_path,
                    ) {
                        Ok(()) => true,
                        Err(err) => {
                            let span = match &err {
                                ModuleResolveError::UnknownModule(_) => {
                                    group.first_path_segment_span()
                                }
                                ModuleResolveError::NotEnoughSegments => group.node().span(),
                            };
                            let source_span = self.source_span(source_id, span);
                            error_failed_to_resolve_import(
                                self.session,
                                source_id,
                                source_span,
                                &err,
                            );
                            false
                        }
                    };

                    let target_source = if file_path_resolved {
                        match self.canonicalize_import(source_id, group.file_path_span()) {
                            Some(target_path) => self.resolve_or_parse_source(target_path),
                            None => None,
                        }
                    } else {
                        None
                    };

                    for item in group.items() {
                        if let (Some(target_source), Some(name), Some(name_span)) =
                            (target_source, item.name(), item.name_span())
                        {
                            let imported_as = item.alias().unwrap_or(name);
                            let prev =
                                self.file_imports[source_id][import_idx].replace(FileImport {
                                    kind: ImportKind::Specific {
                                        selected_name: name,
                                        imported_as,
                                        name_span,
                                    },
                                    target_source,
                                    span: item.span(),
                                });
                            assert!(prev.is_none());
                        }
                        import_idx += 1;
                    }
                }
                _ => {}
            }
        }

        assert!(self.parsed_sources[source_id].1.replace(cst).is_none());

        Some(source_id)
    }

    fn canonicalize_import(
        &mut self,
        source_id: SourceId,
        file_path_span: TokenSpan,
    ) -> Option<PathBuf> {
        match self.fs.canonicalize(&self.import_resolved_path) {
            Ok(path) => Some(path),
            Err(err) => {
                let source_span = self.source_span(source_id, file_path_span);
                error_failed_to_canonicalize_import(
                    self.session,
                    source_id,
                    source_span,
                    &self.import_resolved_path,
                    &err,
                );
                None
            }
        }
    }

    fn resolve_or_parse_source(&mut self, target_path: PathBuf) -> Option<SourceId> {
        match self.path_to_source.get(&target_path) {
            Some(&id) => Some(id),
            None => self.parse_source(target_path),
        }
    }

    fn source_span(&self, source_id: SourceId, span: TokenSpan) -> SourceSpan {
        self.parsed_sources[source_id].0.tokens_src_span(span)
    }
}

pub fn parse_project(
    entry_path: &Path,
    module_resolver: &ModuleResolver,
    session: &mut Session,
    fs: &impl SourceFs,
) -> Option<ParsedProject> {
    let entry_path = match fs.canonicalize(entry_path) {
        Ok(path) => path,
        Err(err) => {
            error_failed_to_canonicalize_entry(session, entry_path, &err);
            return None;
        }
    };

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

    assert_eq!(parser.parse_source(entry_path)?, SourceId::ROOT);

    Some(ParsedProject {
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
                imports.push_iter(file_imports.iter().filter_map(|&i| i));
            }
            imports
        },
    })
}
