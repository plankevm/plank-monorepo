use crate::Evaluator;
use plank_session::{builtins::builtin_names, diagnostic::fmt_count, *};

impl Evaluator<'_> {
    pub fn emit_type_mismatch_error(
        &mut self,
        expected_ty: TypeId,
        expected_loc: SrcLoc,
        actual_ty: TypeId,
        actual_loc: SrcLoc,
    ) {
        let primary_label = format!(
            "expected `{}`, got `{}`",
            self.types.format(self.session, expected_ty),
            self.types.format(self.session, actual_ty),
        );
        let secondary_label =
            format!("`{}` expected because of this", self.types.format(self.session, expected_ty),);
        let diagnostic = Diagnostic::error("mismatched types").cross_source_annotations(
            actual_loc,
            primary_label,
            expected_loc,
            secondary_label,
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_type_constraint_not_type(&mut self, ty: TypeId, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("value used as type").primary(
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
        let primary_label = format!(
            "expected `{}`, got `{}`",
            self.types.format(self.session, ty1),
            self.types.format(self.session, ty2),
        );
        let secondary_label =
            format!("`{}` expected because of this", self.types.format(self.session, ty1));
        let diagnostic = Diagnostic::error("`if` and `else` have incompatible types")
            .cross_source_annotations(loc2, primary_label, loc1, secondary_label);
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_arg_count_mismatch(
        &mut self,
        expected: usize,
        actual: usize,
        call_loc: SrcLoc,
        def_loc: SrcLoc,
    ) {
        let call_label = format!("expected {}, got {actual}", fmt_count(expected, "argument"));
        let def_label = format!("defined with {}", fmt_count(expected, "parameter"));
        let diagnostic = Diagnostic::error("wrong number of arguments")
            .cross_source_annotations(call_loc, call_label, def_loc, def_label);
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_call_target_not_comptime(&mut self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("call target must be known at compile time")
            .primary(loc.source, loc.span, "not known at compile time")
            .note("function calls are statically dispatched");
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_closure_capture_not_comptime(&mut self, use_loc: SrcLoc, def_loc: SrcLoc) {
        let diagnostic = Diagnostic::error("closure capture must be known at compile time")
            .cross_source_annotations(
                use_loc,
                "captures a runtime value",
                def_loc,
                "not known at compile time",
            )
            .note("closures can only capture values known at compile time");
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_comptime_local_not_available(&mut self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("comptime block capture must be known at compile time")
            .primary(loc.source, loc.span, "not known at compile time")
            .note("comptime blocks can only reference values known at compile time");
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_type_not_comptime(&mut self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("struct type must be known at compile time").primary(
            loc.source,
            loc.span,
            "not known at compile time",
        );
        self.session.emit_diagnostic(diagnostic);
    }

    pub(crate) fn emit_not_yet_implemented(&mut self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("not yet implemented")
            .element(Annotations::new(loc.source).no_label(loc.span, AnnotationKind::Primary));
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_no_matching_builtin_signature(
        &mut self,
        builtin: EvmBuiltin,
        arg_types: &[TypeId],
        loc: SrcLoc,
    ) {
        use std::fmt::Write;

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

        let (title, label) = if builtin.signatures()[0].0.len() == arg_types.len() {
            let mut args_str = String::new();
            for (i, &ty) in arg_types.iter().enumerate() {
                if i > 0 {
                    args_str.push_str(", ");
                }
                let _ = write!(args_str, "{}", self.types.format(self.session, ty));
            }
            (
                "no valid match for builtin signature",
                format!("`{builtin}` cannot be called with ({args_str})"),
            )
        } else {
            let expected = builtin.signatures()[0].0.len();
            (
                "wrong number of arguments",
                format!(
                    "`{builtin}` called with {}, but requires {}",
                    fmt_count(arg_types.len(), "argument"),
                    expected,
                ),
            )
        };

        let diagnostic = Diagnostic::error(title).primary(loc.source, loc.span, label).note(note);
        self.session.emit_diagnostic(diagnostic);
    }

    pub fn emit_unsupported_eval_of_evm_builtin(&mut self, builtin: EvmBuiltin, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("comptime evaluation not supported").primary(
            loc.source,
            loc.span,
            format!("`{}` cannot be evaluated at compile time", builtin.name()),
        );
        self.session.emit_diagnostic(diagnostic);
    }
}
