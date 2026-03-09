use plank_parser::{
    FILE_EXTENSION, PlankInterner,
    error_report::ErrorCollector,
    module::ModuleResolver,
    project::{ParsedProject, parse_project},
    source_fs::InMemoryFs,
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
    entry_path: PathBuf,
    fs: InMemoryFs,
}

impl TestProject {
    pub fn single(source: &str) -> Self {
        let entry_name = format!("main.{FILE_EXTENSION}");
        let mut fs = InMemoryFs::new();
        fs.add_file(&entry_name, dedent_preserve_indent(source));
        Self { entry_path: PathBuf::from(entry_name), fs }
    }

    pub fn build(self, interner: &mut PlankInterner) -> Result<ParsedProject, ErrorCollector> {
        let module_resolver = ModuleResolver::default();
        let mut collector = ErrorCollector::default();

        let project =
            parse_project(&self.entry_path, &module_resolver, interner, &mut collector, &self.fs);

        if !collector.errors.is_empty() {
            return Err(collector);
        }
        Ok(project)
    }
}
