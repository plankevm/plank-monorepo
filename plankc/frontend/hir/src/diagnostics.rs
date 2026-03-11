use plank_diagnostics::{Diagnostic, DiagnosticContext};

use crate::BlockLowerer;

impl<'a, D: DiagnosticContext> BlockLowerer<'a, D> {
    pub(crate) fn emit_diagnostic(&self, diagnostic: Diagnostic) {
        self.diag_ctx.borrow_mut().emit(diagnostic);
    }
}
