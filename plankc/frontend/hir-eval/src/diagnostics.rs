use crate::Evaluator;
use plank_session::{builtins::builtin_names, diagnostic::fmt_count, *};

impl Evaluator<'_> {
    pub fn emit_type_mismatch_error(
        &self,
        expected_ty: TypeId,
        expected_loc: SrcLoc,
        actual_ty: TypeId,
        actual_loc: SrcLoc,
    ) {
        let mut session = self.session.borrow_mut();
        let primary_label = format!(
            "expected `{}`, got `{}`",
            self.types.format(&session, expected_ty),
            self.types.format(&session, actual_ty),
        );
        let secondary_label =
            format!("`{}` expected because of this", self.types.format(&session, expected_ty),);
        let diagnostic = Diagnostic::error("mismatched types").cross_source_annotations(
            actual_loc,
            primary_label,
            expected_loc,
            secondary_label,
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_type_constraint_not_type(&self, ty: TypeId, loc: SrcLoc) {
        let mut session = self.session.borrow_mut();
        let diagnostic = Diagnostic::error("value used as type").primary(
            loc.source,
            loc.span,
            format!(
                "expected {}, got value of type `{}`",
                builtin_names::TYPE,
                self.types.format(&session, ty)
            ),
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_type_mismatch_simple(&self, expected_ty: TypeId, actual_ty: TypeId, loc: SrcLoc) {
        let mut session = self.session.borrow_mut();
        let diagnostic = Diagnostic::error("mismatched types").primary(
            loc.source,
            loc.span,
            format!(
                "expected `{}`, got `{}`",
                self.types.format(&session, expected_ty),
                self.types.format(&session, actual_ty),
            ),
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_not_a_struct_type(&self, ty: TypeId, loc: SrcLoc) {
        let mut session = self.session.borrow_mut();
        let diagnostic = Diagnostic::error("expected struct type").primary(
            loc.source,
            loc.span,
            format!("`{}` is not a struct type", self.types.format(&session, ty)),
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_member_on_non_struct(&self, ty: TypeId, loc: SrcLoc) {
        let mut session = self.session.borrow_mut();
        let diagnostic = Diagnostic::error("no fields on type").primary(
            loc.source,
            loc.span,
            format!("value of type `{}` is not a struct type", self.types.format(&session, ty)),
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_not_callable(&self, ty: TypeId, loc: SrcLoc) {
        let mut session = self.session.borrow_mut();
        let diagnostic = Diagnostic::error("expected function").primary(
            loc.source,
            loc.span,
            format!("`{}` is not callable", self.types.format(&session, ty)),
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_incompatible_branch_types(
        &self,
        ty1: TypeId,
        loc1: SrcLoc,
        ty2: TypeId,
        loc2: SrcLoc,
    ) {
        let mut session = self.session.borrow_mut();
        let primary_label = format!(
            "expected `{}`, got `{}`",
            self.types.format(&session, ty1),
            self.types.format(&session, ty2),
        );
        let secondary_label =
            format!("`{}` expected because of this", self.types.format(&session, ty1));
        let diagnostic = Diagnostic::error("`if` and `else` have incompatible types")
            .cross_source_annotations(loc2, primary_label, loc1, secondary_label);
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_arg_count_mismatch(
        &self,
        expected: usize,
        actual: usize,
        call_loc: SrcLoc,
        def_loc: SrcLoc,
    ) {
        let call_label = format!("expected {}, got {actual}", fmt_count(expected, "argument"));
        let def_label = format!("defined with {}", fmt_count(expected, "parameter"));
        let diagnostic = Diagnostic::error("wrong number of arguments")
            .cross_source_annotations(call_loc, call_label, def_loc, def_label);
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub fn emit_call_target_not_comptime(&self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("call target must be known at compile time")
            .primary(loc.source, loc.span, "not known at compile time")
            .note("function calls are statically dispatched");
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub fn emit_closure_capture_not_comptime(&self, use_loc: SrcLoc, def_loc: SrcLoc) {
        let diagnostic = Diagnostic::error("closure capture must be known at compile time")
            .cross_source_annotations(
                use_loc,
                "captures a runtime value",
                def_loc,
                "not known at compile time",
            )
            .note("closures can only capture values known at compile time");
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_field_not_comptime(&self, field_name: StrId, field_loc: SrcLoc) {
        let mut session = self.session.borrow_mut();
        let diagnostic = Diagnostic::error("struct field must be known at compile time").primary(
            field_loc.source,
            field_loc.span,
            format!("value of `{}` is not known at compile time", session.lookup_name(field_name),),
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_type_index_not_comptime(&self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("struct definition requires compile-time values")
            .primary(loc.source, loc.span, "type index is not known at compile time");
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_field_type_not_comptime(&self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("struct definition requires compile-time values")
            .primary(loc.source, loc.span, "field type is not known at compile time");
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub fn emit_comptime_local_not_available(&self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("comptime block capture must be known at compile time")
            .primary(loc.source, loc.span, "not known at compile time")
            .note("comptime blocks can only reference values known at compile time");
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_type_not_comptime(&mut self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("struct type must be known at compile time").primary(
            loc.source,
            loc.span,
            "not known at compile time",
        );
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub(crate) fn emit_not_yet_implemented(&self, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("not yet implemented")
            .element(Annotations::new(loc.source).no_label(loc.span, AnnotationKind::Primary));
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub fn emit_no_matching_builtin_signature(
        &self,
        builtin: EvmBuiltin,
        arg_types: &[TypeId],
        loc: SrcLoc,
    ) {
        use std::fmt::Write;

        let mut session = self.session.borrow_mut();
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
                let _ = write!(note, "{}", self.types.format(&session, ty));
            }
            note.push(')');
        }

        let (title, label) = if builtin.signatures()[0].0.len() == arg_types.len() {
            let mut args_str = String::new();
            for (i, &ty) in arg_types.iter().enumerate() {
                if i > 0 {
                    args_str.push_str(", ");
                }
                let _ = write!(args_str, "{}", self.types.format(&session, ty));
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
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_unsupported_eval_of_evm_builtin(&self, builtin: EvmBuiltin, loc: SrcLoc) {
        let diagnostic = Diagnostic::error("comptime evaluation not supported").primary(
            loc.source,
            loc.span,
            format!("`{}` cannot be evaluated at compile time", builtin.name()),
        );
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_lit_unexpected_field(
        &self,
        struct_ty: TypeId,
        lit_loc: SrcLoc,
        field_name: StrId,
        field_offset: SourceByteOffset,
    ) {
        let mut session = self.session.borrow_mut();
        let (field, field_span) = session.lookup_name_spanned(field_name, field_offset);
        let diagnostic = Diagnostic::error("unexpected field").primary(
            lit_loc.source,
            field_span,
            format!("`{}` has no field `{field}`", self.types.format(&session, struct_ty)),
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_unknown_field_access(
        &self,
        struct_ty: TypeId,
        expr_loc: SrcLoc,
        field_name: StrId,
    ) {
        let mut session = self.session.borrow_mut();
        let diagnostic = Diagnostic::error("unknown field").primary(
            expr_loc.source,
            expr_loc.span,
            format!(
                "`{}` has no field `{}`",
                self.types.format(&session, struct_ty),
                session.lookup_name(field_name),
            ),
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_duplicate_field(
        &self,
        field_name: StrId,
        lit_loc: SrcLoc,
        first: SourceByteOffset,
        duplicate: SourceByteOffset,
    ) {
        let mut session = self.session.borrow_mut();
        let (field, first_span) = session.lookup_name_spanned(field_name, first);
        let (_, duplicate_span) = session.lookup_name_spanned(field_name, duplicate);

        let diagnostic = Diagnostic::error("duplicate field").cross_source_annotations(
            SrcLoc::new(lit_loc.source, duplicate_span),
            format!("`{field}` assigned more than once"),
            SrcLoc::new(lit_loc.source, first_span),
            "first assigned here",
        );
        session.emit_diagnostic(diagnostic);
    }

    pub fn emit_struct_missing_field(&self, struct_ty: TypeId, field_name: StrId, lit_loc: SrcLoc) {
        let mut session = self.session.borrow_mut();
        let diagnostic = Diagnostic::error("missing field").primary(
            lit_loc.source,
            lit_loc.span,
            format!(
                "missing field `{}` in `{}`",
                session.lookup_name(field_name),
                self.types.format(&session, struct_ty),
            ),
        );
        session.emit_diagnostic(diagnostic);
    }
}
