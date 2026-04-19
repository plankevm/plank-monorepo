mod basic;
mod calls;
mod comptime;
mod logical_ops;
mod structs;
mod types;

use plank_mir::{Mir, display::DisplayMir};
use plank_session::Session;
use plank_test_utils::{TestProject, dedent_preserve_blank_lines};
use plank_values::ValueInterner;

fn try_lower(project: impl Into<TestProject>) -> (Mir, ValueInterner, Session) {
    let project = project.into();
    let mut session = Session::new();
    let project = project.build(&mut session);

    let mut big_nums = ValueInterner::new();
    let hir = plank_hir::lower(&project, &mut big_nums, &mut session);
    let mir = crate::evaluate(&hir, &mut big_nums, &mut session);

    (mir, big_nums, session)
}

fn assert_lowers_to(source: &str, expected: &str) {
    assert_project_lowers_to(source, expected)
}

fn assert_project_lowers_to(project: impl Into<TestProject>, expected: &str) {
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

fn render_project_diagnostics(test_project: impl Into<TestProject>) -> Vec<String> {
    let (_, _, session) = try_lower(test_project);
    session.diagnostics().iter().map(|d| d.render_plain(&session)).collect()
}

#[track_caller]
fn assert_diagnostics(source: impl Into<TestProject>, expected: &[&str]) {
    assert_project_diagnostics(source, expected)
}

#[track_caller]
fn assert_project_diagnostics(test_project: impl Into<TestProject>, expected: &[&str]) {
    let actual = render_project_diagnostics(test_project);
    let expected: Vec<String> =
        expected.iter().map(|s| dedent_preserve_blank_lines(s).trim().to_string()).collect();
    let actual: Vec<String> = actual.iter().map(|s| s.trim().to_string()).collect();

    let message = if actual.len() != expected.len() {
        format!("length mismatch: {} != {}", actual.len(), expected.len())
    } else {
        "".to_string()
    };
    let actual_joined = actual.join("\n\n---\n\n");
    let expected_joined = expected.join("\n\n---\n\n");
    pretty_assertions::assert_str_eq!(actual_joined, expected_joined, "{}", message);
    assert_eq!(actual.len(), expected.len(), "length mismatch, actual != expected");
}
