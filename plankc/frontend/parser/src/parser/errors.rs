use plank_session::{Diagnostic, SourceSpan};

use crate::{
    lexer::{ErrorToken, Token, TokenIdx},
    parser::Parser,
};

impl<'a> Parser<'a> {
    pub(crate) fn emit_lexer_error(&mut self, error: ErrorToken, ti: TokenIdx) {
        let span = self.tokens.token_src_span(ti);
        let snippet = &self.source[span.usize_range()];
        let mut diagnostic = Diagnostic::error(match error {
            ErrorToken::InvalidChar => format!("invalid character `{}`", snippet.escape_debug()),
            ErrorToken::MalformedIdent => format!("malformed literal or identifier `{}`", snippet),
            ErrorToken::UnclosedBlockComment => "unclosed block comment".to_string(),
        })
        .span(self.source_id, span, plank_session::diagnostic::AnnotationStyle::Primary);

        match error {
            ErrorToken::MalformedIdent => {
                diagnostic =
                    diagnostic.help("valid identifiers must match [a-zA-Z_][a-zA-Z0-9_]*").help(
                        "valid number literals must match one of -?[0-9][0-9_]* or\
 -?0x[0-9A-Fa-f][0-9A-Fa-f_]* or -?0b[01][01_]*",
                    );
            }
            ErrorToken::InvalidChar => {}
            ErrorToken::UnclosedBlockComment => {
                diagnostic = diagnostic
                    .help("block comments may be nested, every `/*` must be matched by a `*/`");
            }
        }

        self.session.emit_diagnostic(diagnostic);
    }

    pub(crate) fn emit_unexpected_token(&mut self, found: Token, span: SourceSpan) {
        use std::fmt::Write;
        let mut label = String::with_capacity(30 + self.expected.len() * 12);
        write!(&mut label, "unexpected {}, expected ", found).unwrap();
        match self.expected.as_slice() {
            &[] => write!(&mut label, "nothing").unwrap(),
            &[single] => write!(&mut label, "{}", single).unwrap(),
            [first, rest @ ..] => {
                write!(&mut label, "one of {}", first).unwrap();
                for token in rest {
                    write!(&mut label, ", {}", token).unwrap();
                }
            }
        }
        let diagnostic =
            Diagnostic::error(format!("unexpected {}", found)).primary(self.source_id, span, label);
        self.session.emit_diagnostic(diagnostic);
    }

    pub(crate) fn emit_missing_token(&mut self, missing: Token, span: SourceSpan) {
        use std::fmt::Write;
        let mut label = String::with_capacity(30 + self.expected.len() * 12);
        write!(&mut label, "missing {}", missing).unwrap();
        match self.expected.as_slice() {
            &[] => write!(&mut label, ", expected nothing").unwrap(),
            &[single] => assert!(single == missing),
            [first, rest @ ..] => {
                write!(&mut label, "one of {}", first).unwrap();
                for token in rest {
                    write!(&mut label, ", {}", token).unwrap();
                }
            }
        }
        let diagnostic =
            Diagnostic::error(format!("missing {}", missing)).primary(self.source_id, span, label);
        self.session.emit_diagnostic(diagnostic);
    }

    pub(crate) fn emit_missing_specific(&mut self, missing: Token, span: SourceSpan) {
        let diagnostic = Diagnostic::error(format!("missing {}", missing)).span(
            self.source_id,
            span,
            plank_session::diagnostic::AnnotationStyle::Primary,
        );
        self.session.emit_diagnostic(diagnostic);
    }
}
