use crate::{
    StrId,
    ast::{File, ImportKind, TopLevelDef},
    cst::ConcreteSyntaxTree,
    diagnostics::DiagnosticsContext,
    interner::PlankInterner,
    lexer::Lexed,
    module::ModuleManager,
    parser::parse,
    source::{SourceId, SourceManager},
};
use hashbrown::HashMap;
use sensei_core::{IndexVec, list_of_lists::ListOfLists};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};

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

pub fn parse_project(
    entry_path: &Path,
    module_manager: &ModuleManager,
    interner: &mut PlankInterner,
    diagnostics: &mut impl DiagnosticsContext,
) -> ParsedProject {
    let mut source_manager = SourceManager::default();
    let mut sources: IndexVec<SourceId, String> = IndexVec::new();
    let mut csts: IndexVec<SourceId, ConcreteSyntaxTree> = IndexVec::new();
    let mut imports: ListOfLists<SourceId, FileImport> = ListOfLists::new();
    let mut path_to_source: HashMap<PathBuf, SourceId> = HashMap::new();
    let mut pending: VecDeque<SourceId> = VecDeque::new();
    let mut segment_buf: Vec<StrId> = Vec::new();

    let entry_path = entry_path.canonicalize().expect("failed to canonicalize entry path");
    let entry = source_manager.add_source(entry_path.clone());
    path_to_source.insert(entry_path, entry);
    pending.push_back(entry);

    while let Some(source_id) = pending.pop_front() {
        let source = std::fs::read_to_string(&source_manager[source_id].path)
            .expect("failed to read source file");
        let cst = parse(&Lexed::lex(&source), interner, diagnostics);

        imports.push_with(|mut list| {
            for def in File::new(cst.file_view()).expect("failed to init file from CST").iter_defs()
            {
                let TopLevelDef::Import(import) = def else { continue };

                import.collect_path_segments(&mut segment_buf);
                let resolved = module_manager
                    .resolve(&segment_buf, import.is_glob(), interner)
                    .expect("failed to resolve import");

                let target_path =
                    resolved.file_path.canonicalize().expect("failed to canonicalize import path");
                let target_source =
                    *path_to_source.entry(target_path.clone()).or_insert_with(|| {
                        let id = source_manager.add_source(target_path);
                        pending.push_back(id);
                        id
                    });

                match import.kind {
                    Some(ImportKind::As(alias)) => {
                        list.push(FileImport {
                            local_name: Some(alias),
                            target_source,
                            target_const: resolved.const_name,
                        });
                    }
                    Some(ImportKind::All) => {
                        list.push(FileImport {
                            local_name: None,
                            target_source,
                            target_const: None,
                        });
                    }
                    None => {
                        let const_name =
                            resolved.const_name.expect("non-glob import has const name");
                        list.push(FileImport {
                            local_name: Some(const_name),
                            target_source,
                            target_const: Some(const_name),
                        });
                    }
                }
            }
        });

        diagnostics.finish_source(source_id);
        sources.push(source);
        csts.push(cst);
    }

    ParsedProject { source_manager, sources, csts, imports, entry }
}
