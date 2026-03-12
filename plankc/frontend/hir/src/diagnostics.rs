use plank_core::Span;
use plank_diagnostics::{Diagnostic, DiagnosticsContext};
use plank_parser::lexer::TokenIdx;

use crate::{BlockLowerer, StrId};

impl<'a, D: DiagnosticsContext> BlockLowerer<'a, D> {
    pub(crate) fn emit_diagnostic(&self, diagnostic: Diagnostic) {
        self.diag_ctx.borrow_mut().emit(diagnostic);
    }

    pub(crate) fn emit_unresolved_identifier(&self, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let diagnostic = Diagnostic::error(format!(
            "unresolved identifier '{}'",
            &self.interner[name]
        ))
        .primary(self.source_id, source_span, "not found in this scope");
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn emit_assignment_to_immutable(&self, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let diagnostic = Diagnostic::error(format!(
            "variable '{}' was not declared mutable",
            &self.interner[name]
        ))
        .primary(self.source_id, source_span, "assignment to immutable variable")
        .help("consider declaring it with `let mut`");
        self.emit_diagnostic(diagnostic);
    }
}
