use plank_session::{Diagnostic, Session};
use plank_source::{
    FILE_EXTENSION, ModuleResolver, ParsedProject, parse_project, source_fs::InMemoryFs,
};
use std::path::{Path, PathBuf};

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
    modules: Vec<(String, PathBuf)>,
    core_ops_path: Option<PathBuf>,
}

impl From<&str> for TestProject {
    fn from(source: &str) -> Self {
        Self::root(source)
    }
}

impl TestProject {
    pub fn root(source: &str) -> Self {
        let entry_name = format!("main.{FILE_EXTENSION}");
        let mut fs = InMemoryFs::new();
        fs.add_file(&entry_name, dedent_preserve_indent(source));
        Self { entry_path: PathBuf::from(entry_name), fs, modules: Vec::new(), core_ops_path: None }
    }

    pub fn add_file(mut self, name: &str, source: &str) -> Self {
        let path = format!("{name}.{FILE_EXTENSION}");
        self.fs.add_file(&path, dedent_preserve_indent(source));
        self
    }

    pub fn add_module(mut self, name: &str, root: impl Into<PathBuf>) -> Self {
        self.modules.push((name.to_string(), root.into()));
        self
    }

    pub fn with_core_ops(mut self, source: &str) -> Self {
        let path = PathBuf::from("__core_ops.plk");
        self.fs.add_file(&path, dedent_preserve_indent(source));
        self.core_ops_path = Some(path);
        self
    }

    pub fn with_stdlib_dir(mut self, dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let prefix = PathBuf::from("std");

        fn walk(fs: &mut InMemoryFs, dir: &Path, prefix: &Path) {
            for entry in std::fs::read_dir(dir).expect("read stdlib dir") {
                let entry = entry.expect("read dir entry");
                let path = entry.path();
                let name = entry.file_name();
                let fs_path = prefix.join(&name);

                if path.is_dir() {
                    walk(fs, &path, &fs_path);
                } else {
                    let content = std::fs::read_to_string(&path).expect("read stdlib file");
                    fs.add_file(&fs_path, content);
                }
            }
        }

        walk(&mut self.fs, dir, &prefix);
        self.modules.push(("std".to_string(), prefix.clone()));
        self.core_ops_path = Some(prefix.join("core_ops.plk"));
        self
    }

    pub fn build(self, session: &mut Session) -> ParsedProject {
        let mut module_resolver = ModuleResolver::default();
        for (name, root) in self.modules {
            module_resolver
                .register(session.intern(&name), root)
                .expect("module registration succeeds");
        }
        parse_project(
            &self.entry_path,
            self.core_ops_path.as_deref(),
            &module_resolver,
            session,
            &self.fs,
        )
        .expect("project should be parsed")
    }
}

pub fn assert_diagnostics(diagnostics: &[Diagnostic], session: &Session, expected: &[&str]) {
    let actual: Vec<_> = diagnostics.iter().map(|d| d.render_plain(session)).collect();
    let expected: Vec<_> =
        expected.iter().map(|s| dedent_preserve_blank_lines(s).trim().to_string()).collect();

    let err_message = if actual.len() != expected.len() {
        format!("length mismatch: {} != {}", actual.len(), expected.len())
    } else {
        "".to_string()
    };

    let actual_joined = actual.join("\n\n---\n\n");
    let expected_joined = expected.join("\n\n---\n\n");
    pretty_assertions::assert_str_eq!(actual_joined, expected_joined, "{err_message}");
    assert_eq!(actual.len(), expected.len(), "length mismatch, actual != expected");
}
