use crate::Evaluator;
use plank_session::{builtins::builtin_names, builtins::Builtin, *};

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
                .secondary(
                    expected_loc.span,
                    format!(
                        "`{}` expected because of this",
                        self.types.format(self.session, expected_ty),
                    ),
                ),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_type_constraint_not_type(&mut self, ty: TypeId, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("type constraint not type").primary(
            loc.source,
            loc.span,
            format!(
                "expected {}, got value of type `{}`",
                builtin_names::TYPE,
                self.types.format(self.session, ty)
            ),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_type_mismatch_simple(
        &mut self,
        expected_ty: TypeId,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        let diagnostic = Diagnostic::error("mismatched types").primary(
            loc.source,
            loc.span,
            format!(
                "expected `{}`, got `{}`",
                self.types.format(self.session, expected_ty),
                self.types.format(self.session, actual_ty),
            ),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_not_a_struct_type(&mut self, ty: TypeId, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("expected struct type").primary(
            loc.source,
            loc.span,
            format!("`{}` is not a struct type", self.types.format(self.session, ty)),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_member_on_non_struct(&mut self, ty: TypeId, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("no fields on type").primary(
            loc.source,
            loc.span,
            format!("value of type `{}` is not a struct type", self.types.format(self.session, ty)),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_not_callable(&mut self, ty: TypeId, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("expected function").primary(
            loc.source,
            loc.span,
            format!("`{}` is not callable", self.types.format(self.session, ty)),
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
                .secondary(
                    loc1.span,
                    format!("`{}` expected because of this", self.types.format(self.session, ty1)),
                ),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_arg_count_mismatch(&mut self, expected: usize, actual: usize, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("wrong number of arguments").primary(
            loc.source,
            loc.span,
            format!("expected {expected} argument(s), got {actual}"),
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_not_known_at_comptime(&mut self, what: &str, loc: SrcLoc) {
        let diagnostic = Diagnostic::error(format!("{what} must be known at compile time"))
            .primary(loc.source, loc.span, "not known at compile time");
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_closure_capture_not_comptime(
        &mut self,
        closure_loc: SrcLoc,
        capture_loc: SrcLoc,
    ) {
        assert_eq!(closure_loc.source, capture_loc.source);
        let diagnostic = Diagnostic::error("closure capture must be known at compile time")
            .element(
                Annotations::new(closure_loc.source)
                    .primary(closure_loc.span, "closure captures a runtime value")
                    .secondary(capture_loc.span, "not known at compile time"),
            )
            .note("closures can only capture values known at compile time");
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_no_matching_builtin_signature(
        &mut self,
        builtin: Builtin,
        arg_types: &[TypeId],
        loc: SrcLoc,
    ) {
        use std::fmt::Write;
        let mut args_str = String::new();
        for (i, &ty) in arg_types.iter().enumerate() {
            if i > 0 {
                args_str.push_str(", ");
            }
            let _ = write!(args_str, "{}", self.types.format(self.session, ty));
        }

        let mut note = format!("`{builtin}` accepts ");
        for (i, &(params, _ret)) in builtin.signatures().iter().enumerate() {
            if i > 0 {
                note.push_str(", ");
            }
            note.push('(');
            for (j, &ty) in params.iter().enumerate() {
                if j > 0 {
                    note.push_str(", ");
                }
                let _ = write!(note, "{}", self.types.format(self.session, ty));
            }
            note.push(')');
        }

        let diagnostic = Diagnostic::error("no valid match for builtin signature")
            .primary(
                loc.source,
                loc.span,
                format!("`{builtin}` cannot be called with ({args_str})"),
            )
            .note(note);
        self.session.emit_diagnostic(diagnostic);
    }
}
