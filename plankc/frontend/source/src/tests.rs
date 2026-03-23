use crate::{FILE_EXTENSION, ModuleResolver, parse_project, source_fs::RealFs};
use plank_session::Session;
use std::path::PathBuf;

fn source_file(name: &str) -> String {
    format!("{name}.{FILE_EXTENSION}")
}

fn write_files(dir: &std::path::Path, files: &[(&str, &str)]) {
    for &(name, content) in files {
        std::fs::write(dir.join(source_file(name)), content).unwrap();
    }
}

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

    let mut session = Session::new();
    let mut modules = ModuleResolver::default();
    modules
        .register(session.intern("m"), dir.path().to_path_buf())
        .expect("module registration succeeds");

    let project =
        parse_project(&dir.path().join(source_file("main")), &modules, &mut session, &RealFs)
            .expect("project should be parsed");
    assert!(!session.has_errors(), "parse errors: {:?}", session.diagnostics());

    for (id, _parsed_source) in project.parsed_sources.enumerate_idx() {
        let source = session.get_source(id);
        let expected = std::fs::read_to_string(&source.path).unwrap();
        assert_eq!(
            source.content,
            expected,
            "sources[{id:?}] does not match {}",
            source.path.display()
        );
    }
}

#[test]
fn duplicate_module_registration_is_error() {
    let mut session = Session::new();
    let mut modules = ModuleResolver::default();
    let name_id = session.intern("m");
    modules.register(name_id, PathBuf::from("/a")).expect("first registration succeeds");
    modules.register(name_id, PathBuf::from("/b")).expect_err("duplicate registration should fail");
}
