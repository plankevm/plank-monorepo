use plank_diagnostics::{Diagnostic, DiagnosticsContext};

use crate::BlockLowerer;

impl<'a, D: DiagnosticsContext> BlockLowerer<'a, D> {
    #[allow(dead_code)] // TODO: Implement
    pub(crate) fn emit_diagnostic(&self, diagnostic: Diagnostic) {
        self.diag_ctx.borrow_mut().emit(diagnostic);
    }
}
