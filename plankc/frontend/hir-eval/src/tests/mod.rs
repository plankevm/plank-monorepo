mod basic;
mod calls;
mod compile_log;
mod comptime;
mod logical_ops;
mod operators;
mod structs;
mod tuples;
mod types;

use plank_mir::{Mir, display::DisplayMir};
use plank_session::Session;
use plank_test_utils::{TestProject, dedent_preserve_blank_lines};
use plank_values::ValueInterner;

const STD_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../std");

fn std_project(source: &str) -> TestProject {
    TestProject::root(source).with_stdlib_dir(STD_DIR)
}

fn try_lower(project: impl Into<TestProject>) -> (Mir, ValueInterner, Session) {
    let project = project.into();
    let mut session = Session::new();
    let project = project.build(&mut session);
    let evm_spec_id = Default::default();

    let mut big_nums = ValueInterner::new();
    let hir = plank_hir::lower(&project, &mut big_nums, &mut session);
    let mir =
        crate::evaluate(&hir, project.core_ops_source, &mut big_nums, &mut session, evm_spec_id);

    (mir, big_nums, session)
}

#[track_caller]
fn assert_lowers_to(project: impl Into<TestProject>, expected: &str) {
    let (mir, big_nums, session) = try_lower(project);

    if session.has_errors() {
        let diags: Vec<String> =
            session.diagnostics().iter().map(|d| d.render_plain(&session)).collect();
        panic!("expected no diagnostics but got {}:\n{}", diags.len(), diags.join("\n---\n"));
    }

    let actual = format!("{}", DisplayMir::new(&mir, &big_nums, &session));
    let expected = dedent_preserve_blank_lines(expected);

    pretty_assertions::assert_str_eq!(actual.trim(), expected.trim());
}

#[track_caller]
fn assert_diagnostics(source: impl Into<TestProject>, expected: &[&str]) {
    assert_project_diagnostics(source, expected)
}

#[track_caller]
fn assert_project_diagnostics(test_project: impl Into<TestProject>, expected: &[&str]) {
    let (_, _, session) = try_lower(test_project);
    plank_test_utils::assert_diagnostics(session.diagnostics(), &session, expected);
}

#[track_caller]
fn assert_diagnostics_and_compile_logs(
    source: impl Into<TestProject>,
    expected_diagnostics: &[&str],
    expected_logs: &[&str],
) {
    let (_, _, session) = try_lower(source);
    plank_test_utils::assert_diagnostics(session.diagnostics(), &session, expected_diagnostics);

    let actual_logs: Vec<String> =
        session.compile_logs().iter().map(|log| log.to_string()).collect();
    let expected_logs: Vec<String> = expected_logs.iter().map(|s| s.to_string()).collect();
    pretty_assertions::assert_eq!(actual_logs, expected_logs);
}
