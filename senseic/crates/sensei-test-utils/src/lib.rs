use sensei_core::{IndexVec, list_of_lists::ListOfLists};
use sensei_parser::{
    FILE_EXTENSION, PlankInterner,
    error_report::ErrorCollector,
    lexer::Lexed,
    parser::parse,
    project::{FileImport, ParsedProject},
    source::{ROOT_SOURCE, SourceId, SourceManager},
};
use std::path::PathBuf;

/// Strips the minimum common leading whitespace from all non-empty lines,
/// preserving relative indentation. Empty lines are removed.
pub fn dedent_preserve_indent(s: &str) -> String {
    let non_empty_lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();

    if non_empty_lines.is_empty() {
        return String::new();
    }

    let min_indent =
        non_empty_lines.iter().map(|line| line.len() - line.trim_start().len()).min().unwrap_or(0);

    non_empty_lines.iter().map(|line| &line[min_indent..]).collect::<Vec<_>>().join("\n")
}

/// Like [`dedent_preserve_indent`], but keeps blank lines in the output.
pub fn dedent_preserve_blank_lines(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|line| if line.len() > min_indent { &line[min_indent..] } else { line.trim() })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strips all leading whitespace from each line and removes empty lines.
pub fn dedent(s: &str) -> String {
    s.lines().map(|line| line.trim()).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n")
}

/// Builder for creating in-memory test projects without file system access.
pub struct TestProject {
    files: Vec<(String, String)>,
}

impl TestProject {
    pub fn single(source: &str) -> Self {
        let entry_name = format!("main{FILE_EXTENSION}");
        Self { files: vec![(entry_name, dedent_preserve_indent(source))] }
    }

    pub fn build(self, interner: &mut PlankInterner) -> Result<ParsedProject, ErrorCollector> {
        let mut files = self.files.into_iter();
        let (entry_name, entry_content) =
            files.next().expect("test project must have at least one file");
        let mut source_manager = SourceManager::new(PathBuf::from(entry_name));
        let mut sources: IndexVec<SourceId, String> = IndexVec::new();
        let mut csts: IndexVec<SourceId, _> = IndexVec::new();
        let mut imports: ListOfLists<SourceId, FileImport> = ListOfLists::new();
        let mut collector = ErrorCollector::default();

        let lexed = Lexed::lex(&entry_content);

        let cst = parse(&lexed, interner, &mut collector, ROOT_SOURCE);
        imports.push_iter(std::iter::empty());
        sources.push(entry_content);
        csts.push(cst);

        for (filename, content) in files {
            let source_id = source_manager.add_source(PathBuf::from(filename));
            let lexed = Lexed::lex(&content);

            let cst = parse(&lexed, interner, &mut collector, source_id);
            imports.push_iter(std::iter::empty());

            sources.push(content);
            csts.push(cst);
        }

        if !collector.errors.is_empty() {
            return Err(collector);
        }

        Ok(ParsedProject { source_manager, sources, csts, imports, entry: ROOT_SOURCE })
    }
}
