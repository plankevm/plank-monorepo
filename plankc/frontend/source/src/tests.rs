use crate::{FILE_EXTENSION, ModuleResolver, parse_project, source_fs::RealFs};
use plank_parser::{error_report::ErrorCollector, interner::PlankInterner};

fn source_file(name: &str) -> String {
    format!("{name}.{FILE_EXTENSION}")
}

fn write_files(dir: &std::path::Path, files: &[(&str, &str)]) {
    for &(name, content) in files {
        std::fs::write(dir.join(source_file(name)), content).unwrap();
    }
}

/// `sources[id]` must contain the content of the file at `source_manager[id].path`
/// for every source in the project — even when a single file imports multiple others.
#[test]
fn source_content_matches_source_manager_path() {
    let dir = tempfile::tempdir().unwrap();
    write_files(
        dir.path(),
        &[
            ("main", "import m::a::A;\nimport m::b::B;\n\ninit {}\n"),
            ("a", "const A = 1;\n"),
            ("b", "const B = 2;\n"),
        ],
    );

    let mut interner = PlankInterner::default();
    let mut modules = ModuleResolver::default();
    modules.register(interner.intern("m"), dir.path().to_path_buf());

    let mut collector = ErrorCollector::default();
    let project = parse_project(
        &dir.path().join(source_file("main")),
        &modules,
        &mut interner,
        &mut collector,
        &RealFs,
    );
    assert!(collector.errors.is_empty(), "parse errors: {:?}", collector.errors);

    for (id, source) in project.sources.enumerate_idx() {
        let expected = std::fs::read_to_string(&source.path).unwrap();
        assert_eq!(
            source.content,
            expected,
            "sources[{id:?}] does not match {}",
            source.path.display()
        );
    }
}
