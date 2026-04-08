use plank_session::{AnnotationKind, Annotations, ClaimBuilder, Diagnostic, SourceSpan};

use crate::{
    lexer::{ErrorToken, Token, TokenIdx},
    parser::Parser,
};

impl<'a> Parser<'a> {
    pub(crate) fn emit_lexer_error(&mut self, error: ErrorToken, ti: TokenIdx) {
        let span = self.tokens.token_src_span(ti);
        let snippet = &self.source[span.usize_range()];

        let diag = match error {
            ErrorToken::InvalidChar => Diagnostic::error("invalid character").primary(
                self.source_id,
                span,
                match snippet.chars().next().unwrap() {
                    '\'' => "' is not part of any valid syntax construct".to_string(),
                    printable @ '\x20'..='\x7e' => {
                        format!("'{}' is not part of any valid syntax construct", printable)
                    }
                    non_printable => format!(
                        "{} is not a part of any valid syntax construct",
                        non_printable.escape_default()
                    ),
                },
            ),
            ErrorToken::MalformedIdent => Diagnostic::error(
                "malformed number literal or identifier",
            )
            .primary(self.source_id, span, "not a valid identifier or literal")
            .help("identifiers must begin with an ASCII letter or '_'")
            .help("decimal literals may only contain digits 0-9 and '_'")
            .help("hex literals must begin with '0x' and may only contain 0-9, A-F, a-f and '_'")
            .help("binary literals must begin with '0b' and may only contain 0, 1 and '_'"),
            ErrorToken::UnclosedBlockComment => {
                let mut diag = Diagnostic::error("unclosed block comment").primary(
                    self.source_id,
                    span,
                    "missing closing `*/`",
                );
                let mut opening = 0u32;
                let mut chars = snippet.chars().peekable();
                while let Some(c) = chars.next() {
                    match c {
                        '/' if chars.next_if_eq(&'*').is_some() => opening += 1,
                        '*' if chars.next_if_eq(&'/').is_some() => { /* closing */ }
                        _ => {}
                    }
                }
                if opening >= 2 {
                    diag = diag.help(
                        "plank supports nested block comments so each `/*` needs its own `*/`",
                    );
                }
                diag
            }
        };

        self.session.emit_diagnostic(diag);
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
        let diagnostic = Diagnostic::error(format!("missing {}", missing))
            .element(Annotations::new(self.source_id).no_label(span, AnnotationKind::Primary));
        self.session.emit_diagnostic(diagnostic);
    }
}
