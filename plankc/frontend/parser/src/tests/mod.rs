use crate::{cst::display::DisplayCST, lexer::Lexed, parser::parse};
use plank_session::{Session, Source};
use plank_test_utils::{dedent, dedent_preserve_indent};

mod errorless;
mod resiliency;

fn parse_single_source(source: &str, session: &mut Session) -> crate::cst::ConcreteSyntaxTree {
    let source_id =
        session.register_source(Source { path: "test.plk".into(), content: source.to_string() });
    let lexed = Lexed::lex(source);
    parse(session, &lexed, source, source_id)
}

pub fn assert_parser_errors(source: &str, expected_errors: &[&str]) {
    let source = dedent(source);
    let mut session = Session::new();
    let _cst = parse_single_source(&source, &mut session);

    let actual: Vec<String> =
        session.diagnostics().iter().map(|d| d.render_plain(&session).trim().to_string()).collect();

    let expected: Vec<String> = expected_errors.iter().map(|s| dedent_preserve_indent(s)).collect();

    let actual_joined = actual.join("\n\n---\n\n");
    let expected_joined = expected.join("\n\n---\n\n");
    pretty_assertions::assert_str_eq!(actual_joined, expected_joined);
}

pub fn assert_parses_to_cst_no_errors(source: &str, expected: &str) {
    let mut session = Session::new();
    let cst = parse_single_source(source, &mut session);

    if session.has_errors() {
        let formatted: Vec<String> =
            session.diagnostics().iter().map(|d| d.render_plain(&session)).collect();
        panic!(
            "Expected no parser errors, but found {}:\n\n{}",
            session.diagnostics().len(),
            formatted.join("\n\n---\n\n")
        );
    }

    let lexed = Lexed::lex(source);
    let actual = format!("{}", DisplayCST::new(&cst, source, &lexed));

    pretty_assertions::assert_str_eq!(
        actual.trim(),
        expected.trim(),
        "Full tree:\n{}",
        DisplayCST::new(&cst, source, &lexed).show_node_index(true).show_token_spans(true)
    );
}
