use crate::Evaluator;
use plank_session::{builtins::builtin_names, *};

impl Evaluator<'_> {
    pub fn emit_type_mismatch_error(
        &mut self,
        expected_ty: TypeId,
        expected_loc: SrcLoc,
        actual_ty: TypeId,
        actual_loc: SrcLoc,
    ) {
        let source = {
            assert_eq!(expected_loc.source, actual_loc.source);
            expected_loc.source
        };
        let diagnostic = Diagnostic::error("mismatched types").element(
            Annotations::new(source)
                .primary(
                    actual_loc.span,
                    format!(
                        "expected `{}`, got `{}`",
                        self.types.format(self.session, expected_ty),
                        self.types.format(self.session, actual_ty),
                    ),
                )
                .secondary(expected_loc.span, "expected because of this"),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_type_constraint_not_type(&mut self, ty: TypeId, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("type constraint not type").primary(
            loc.source,
            loc.span,
            format!(
                "expected `{}`, got `{}`",
                builtin_names::TYPE,
                self.types.format(self.session, ty)
            ),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_incompatible_branch_types(
        &mut self,
        ty1: TypeId,
        loc1: SrcLoc,
        ty2: TypeId,
        loc2: SrcLoc,
    ) {
        assert_eq!(loc1.source, loc2.source);
        let diagnostic = Diagnostic::error("`if` and `else` have incompatible types").element(
            Annotations::new(loc1.source)
                .primary(
                    loc2.span,
                    format!(
                        "expected `{}`, got `{}`",
                        self.types.format(self.session, ty1),
                        self.types.format(self.session, ty2),
                    ),
                )
                .secondary(loc1.span, "expected because of this"),
        );
        self.session.emit_diagnostic(diagnostic);
    }
}
