use plank_core::{SourceId, SourceSpan, Span};
use plank_diagnostics::{Diagnostic, DiagnosticsContext};
use plank_parser::lexer::TokenIdx;

use crate::StrId;

use super::BlockLowerer;

impl<'a, D: DiagnosticsContext> BlockLowerer<'a, D> {
    pub(crate) fn emit_diagnostic(&self, diagnostic: Diagnostic) {
        self.diag_ctx.borrow_mut().emit(diagnostic);
    }

    pub(crate) fn error_unresolved_identifier(&self, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let diagnostic = Diagnostic::error(format!(
            "unresolved identifier '{}'",
            &self.interner[name]
        ))
        .primary(self.source_id, source_span, "not found in this scope");
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_assignment_to_immutable(&self, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let diagnostic = Diagnostic::error(format!(
            "variable '{}' was not declared mutable",
            &self.interner[name]
        ))
        .primary(self.source_id, source_span, "assignment to immutable variable")
        .help("consider declaring it with `let mut`");
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_multiple_init_blocks(
        &self,
        current: Span<TokenIdx>,
        previous: Span<TokenIdx>,
    ) {
        self.error_multiple_blocks("init", current, previous);
    }

    pub(crate) fn error_multiple_run_blocks(
        &self,
        current: Span<TokenIdx>,
        previous: Span<TokenIdx>,
    ) {
        self.error_multiple_blocks("run", current, previous);
    }

    fn error_multiple_blocks(&self, kind: &str, current: Span<TokenIdx>, previous: Span<TokenIdx>) {
        let diagnostic = Diagnostic::error(format!("multiple {kind} blocks"))
            .primary(
                self.source_id,
                self.lexed.tokens_src_span(current),
                format!("duplicate {kind} block"),
            )
            .secondary(
                self.source_id,
                self.lexed.tokens_src_span(previous),
                format!("previous {kind} block"),
            );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_init_outside_entry(&self, span: Span<TokenIdx>) {
        self.error_outside_entry("init", span);
    }

    pub(crate) fn error_run_outside_entry(&self, span: Span<TokenIdx>) {
        self.error_outside_entry("run", span);
    }

    fn error_outside_entry(&self, kind: &str, span: Span<TokenIdx>) {
        let diagnostic = Diagnostic::error(format!("{kind} not allowed here")).primary(
            self.source_id,
            self.lexed.tokens_src_span(span),
            format!("only the entry file may contain {kind}"),
        );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_shadowing_primitive_type(&self, name: StrId, span: Span<TokenIdx>) {
        self.error_shadowing("primitive type", name, span);
    }

    pub(crate) fn error_shadowing_builtin(&self, name: StrId, span: Span<TokenIdx>) {
        self.error_shadowing("built-in function", name, span);
    }

    fn error_shadowing(&self, kind: &str, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let diagnostic = Diagnostic::error(format!(
            "cannot shadow {kind} '{}'",
            &self.interner[name]
        ))
        .primary(self.source_id, source_span, format!("is a {kind}"));
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_non_call_reference_to_builtin(&self, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let diagnostic = Diagnostic::error(format!(
            "cannot reference built-in function '{}' as a value",
            &self.interner[name]
        ))
        .primary(self.source_id, source_span, "must be called directly");
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_unresolved_import(&self, name: StrId, span: Span<TokenIdx>) {
        let diagnostic = Diagnostic::error(format!("unresolved import '{}'", &self.interner[name]))
            .primary(
                self.source_id,
                self.lexed.tokens_src_span(span),
                "not found in target module",
            );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_import_collision(
        &self,
        name: StrId,
        import_span: Span<TokenIdx>,
        prev_source_id: SourceId,
        prev_source_span: SourceSpan,
    ) {
        let diagnostic = Diagnostic::error(format!(
            "import of '{}' conflicts with existing definition",
            &self.interner[name]
        ))
        .primary(self.source_id, self.lexed.tokens_src_span(import_span), "conflicting import")
        .secondary(prev_source_id, prev_source_span, "previously defined here");
        self.emit_diagnostic(diagnostic);
    }
}
