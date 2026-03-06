use crate::{
    error_report::ErrorCollector, interner::PlankInterner, module::ModuleManager,
    project::parse_project,
};

fn write_files(dir: &std::path::Path, files: &[(&str, &str)]) {
    for &(name, content) in files {
        std::fs::write(dir.join(name), content).unwrap();
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
            ("main.plk", "import m::a::A;\nimport m::b::B;\n\ninit {}\n"),
            ("a.plk", "const A = 1;\n"),
            ("b.plk", "const B = 2;\n"),
        ],
    );

    let mut interner = PlankInterner::default();
    let mut modules = ModuleManager::default();
    modules.register(interner.intern("m"), dir.path().to_path_buf());

    let mut collector = ErrorCollector::default();
    let project =
        parse_project(&dir.path().join("main.plk"), &modules, &mut interner, &mut collector);
    assert!(collector.errors.is_empty(), "parse errors: {:?}", collector.errors);

    for (id, content) in project.sources.enumerate_idx() {
        let path = &project.source_manager[id].path;
        let expected = std::fs::read_to_string(path).unwrap();
        assert_eq!(content, &expected, "sources[{id:?}] does not match {}", path.display());
    }
}
