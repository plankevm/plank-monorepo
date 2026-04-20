use plank_core::Span;
use plank_session::{
    AnnotationKind, Annotations, ClaimBuilder, DiagEmitter, Diagnostic, SourceSpan,
};

use crate::lexer::{ErrorToken, Token, TokenIdx};

use super::Parser;

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
            ErrorToken::AtWithoutIdent => Diagnostic::error("invalid builtin name").primary(
                self.source_id,
                span,
                "expected identifier after `@`",
            ),
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

        diag.emit(self.session);
    }

    pub(crate) fn emit_unexpected_token(&mut self, found: Token, span: SourceSpan) {
        let diagnostic = self.build_unexpected_diagnostic(found, span);
        self.session.emit_diagnostic(diagnostic);
    }

    pub(crate) fn emit_builtin_name_used_as_ident(&mut self, span: SourceSpan) {
        let diagnostic = self
            .build_unexpected_diagnostic(Token::BuiltinName, span)
            .help("`@name` syntax is reserved for builtins and cannot be used as an identifier");
        self.session.emit_diagnostic(diagnostic);
    }

    fn build_unexpected_diagnostic(&self, found: Token, span: SourceSpan) -> Diagnostic {
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
        Diagnostic::error(format!("unexpected {}", found)).primary(self.source_id, span, label)
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
        Diagnostic::error(format!("missing {}", missing))
            .primary(self.source_id, span, label)
            .emit(self.session);
    }

    pub(crate) fn emit_missing_specific(&mut self, missing: Token, span: SourceSpan) {
        Diagnostic::error(format!("missing {}", missing))
            .element(Annotations::new(self.source_id).no_label(span, AnnotationKind::Primary))
            .emit(self.session);
    }

    pub(crate) fn emit_empty_import_group(&mut self, brace_start: TokenIdx) {
        let start = self.tokens.token_src_span(brace_start).start;
        let end = self.last_src_span.end;
        Diagnostic::warning("empty import group")
            .primary(
                self.source_id,
                Span::new(start, end),
                "import group must contain at least one item",
            )
            .emit(self.session);
    }

    pub(crate) fn emit_path_in_import_group(&mut self, path_start: TokenIdx) {
        let start = self.tokens.token_src_span(path_start).start;
        let end = self.last_src_span.end;
        Diagnostic::error("path in import group")
            .primary(
                self.source_id,
                Span::new(start, end),
                "paths are not allowed inside import groups",
            )
            .help("use a separate import statement for items from different submodules")
            .emit(self.session);
    }

    pub(crate) fn emit_glob_in_import_group(&mut self) {
        Diagnostic::error("glob import inside import group")
            .primary(
                self.source_id,
                self.last_src_span,
                "glob imports are not allowed inside import groups",
            )
            .help("use a separate `import foo::*;` statement instead")
            .emit(self.session);
    }

    pub(super) fn emit_unnecessary_braces(&mut self, brace_start: TokenIdx) {
        let start_span = self.tokens.token_src_span(brace_start);
        let end_span = self.last_src_span;
        let src_span = Span::new(start_span.start, end_span.end);
        Diagnostic::warning("unnecessary braces in import")
            .primary(self.source_id, src_span, "this import group contains only one item")
            .help("remove the unnecessary braces")
            .emit(self.session);
    }
}
