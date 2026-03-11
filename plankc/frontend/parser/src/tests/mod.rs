use crate::{
    SourceId,
    cst::display::DisplayCST,
    error_report::{ErrorCollector, LineIndex, format_error},
    interner::PlankInterner,
    lexer::Lexed,
    parser::parse,
};
use plank_test_utils::{dedent, dedent_preserve_indent};

mod errorless;
mod resiliency;

fn parse_single_source(
    source: &str,
    interner: &mut PlankInterner,
) -> (ErrorCollector, crate::cst::ConcreteSyntaxTree) {
    let lexed = Lexed::lex(source);
    let mut collector = ErrorCollector::default();
    let cst = parse(source, &lexed, interner, &mut collector, SourceId::ROOT);
    (collector, cst)
}

pub fn assert_parser_errors(source: &str, expected_errors: &[&str]) {
    let source = dedent(source);
    let mut interner = PlankInterner::default();
    let (collector, _) = parse_single_source(&source, &mut interner);

    let line_index = LineIndex::new(&source);
    let actual: Vec<String> =
        collector.errors.iter().map(|(_, e)| format_error(e, &source, &line_index)).collect();

    let expected: Vec<String> = expected_errors.iter().map(|s| dedent_preserve_indent(s)).collect();

    let actual_joined = actual.join("\n\n---\n\n");
    let expected_joined = expected.join("\n\n---\n\n");
    pretty_assertions::assert_str_eq!(actual_joined, expected_joined);
}

pub fn assert_parses_to_cst_no_errors(source: &str, expected: &str) {
    let mut interner = PlankInterner::default();
    let (collector, cst) = parse_single_source(source, &mut interner);

    if !collector.errors.is_empty() {
        let line_index = LineIndex::new(source);
        let formatted: Vec<String> =
            collector.errors.iter().map(|(_, e)| format_error(e, source, &line_index)).collect();
        panic!(
            "Expected no parser errors, but found {}:\n\n{}",
            collector.errors.len(),
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
