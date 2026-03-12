use plank_core::Span;
use plank_diagnostics::{Diagnostic, DiagnosticsContext};
use plank_parser::lexer::TokenIdx;

use crate::{BlockLowerer, StrId};

impl<'a, D: DiagnosticsContext> BlockLowerer<'a, D> {
    #[allow(dead_code)] // TODO: Implement
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
}
