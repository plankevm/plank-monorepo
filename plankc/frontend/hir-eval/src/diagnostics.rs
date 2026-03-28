use crate::Evaluator;
use plank_session::*;

impl Evaluator<'_> {
    pub fn emit_type_mismatch_error(
        &mut self,
        expected: TypeId,
        produced: TypeId,
        producing_loc: SrcLoc,
    ) {
        let diagnostic = Diagnostic::error("mismatched types").primary(
            producing_loc.source,
            producing_loc.span,
            format!(
                "expected `{}`, got `{}`",
                self.types.format(self.session, expected),
                self.types.format(self.session, produced),
            ),
        );
        self.session.emit_diagnostic(diagnostic);
    }
}
