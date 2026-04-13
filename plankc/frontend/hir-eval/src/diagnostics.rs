use plank_hir as hir;
use plank_session::{builtins::builtin_names, diagnostic::fmt_count, *};
use plank_values::TypeInterner;

pub(crate) struct DiagCtx<'a> {
    pub session: &'a mut Session,
}

impl<'a> DiagCtx<'a> {
    pub fn new(session: &'a mut Session) -> Self {
        Self { session }
    }
}

impl DiagCtx<'_> {
    pub fn emit_type_mismatch(
        &mut self,
        types: &TypeInterner,
        expected_ty: TypeId,
        expected_loc: SrcLoc,
        actual_ty: TypeId,
        actual_loc: SrcLoc,
    ) {
        let primary_label = format!(
            "expected `{}`, got `{}`",
            types.format(self.session, expected_ty),
            types.format(self.session, actual_ty),
        );
        let secondary_label =
            format!("`{}` expected because of this", types.format(self.session, expected_ty));
        Diagnostic::error("mismatched types")
            .cross_source_annotations(actual_loc, primary_label, expected_loc, secondary_label)
            .emit(self.session);
    }

    pub fn emit_type_not_type(&mut self, types: &TypeInterner, ty: TypeId, loc: SrcLoc) {
        Diagnostic::error("value used as type")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "expected {}, got value of type `{}`",
                    builtin_names::TYPE,
                    types.format(self.session, ty)
                ),
            )
            .emit(self.session);
    }

    pub fn emit_struct_literal_field_type_mismatch(
        &mut self,
        types: &TypeInterner,
        expected_ty: TypeId,
        actual_ty: TypeId,
        field_value_loc: SrcLoc,
        field_name: StrId,
    ) {
        let name = self.session.lookup_name(field_name);
        Diagnostic::error("incorrect type for struct field")
            .primary(
                field_value_loc.source,
                field_value_loc.span,
                format!(
                    "field `{name}` expects `{}`, got `{}`",
                    types.format(self.session, expected_ty),
                    types.format(self.session, actual_ty),
                ),
            )
            .emit(self.session);
    }

    pub fn emit_type_mismatch_simple(
        &mut self,
        types: &TypeInterner,
        expected_ty: TypeId,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        Diagnostic::error("mismatched types")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "expected `{}`, got `{}`",
                    types.format(self.session, expected_ty),
                    types.format(self.session, actual_ty),
                ),
            )
            .emit(self.session);
    }

    pub fn emit_not_a_struct_type(&mut self, types: &TypeInterner, ty: TypeId, ty_loc: SrcLoc) {
        Diagnostic::error("expected struct type")
            .primary(
                ty_loc.source,
                ty_loc.span,
                format!("`{}` is not a struct type", types.format(self.session, ty)),
            )
            .emit(self.session);
    }

    pub fn emit_member_on_non_struct(
        &mut self,
        types: &TypeInterner,
        ty: TypeId,
        value_loc: SrcLoc,
    ) {
        Diagnostic::error("no fields on type")
            .primary(
                value_loc.source,
                value_loc.span,
                format!("value of type `{}` is not a struct type", types.format(self.session, ty)),
            )
            .emit(self.session);
    }

    pub fn emit_not_callable(&mut self, types: &TypeInterner, ty: TypeId, loc: SrcLoc) {
        Diagnostic::error("expected function")
            .primary(
                loc.source,
                loc.span,
                format!("`{}` is not callable", types.format(self.session, ty)),
            )
            .emit(self.session);
    }

    pub fn emit_incompatible_branch_types(
        &mut self,
        types: &TypeInterner,
        ty1: TypeId,
        loc1: SrcLoc,
        ty2: TypeId,
        loc2: SrcLoc,
    ) {
        let primary_label = format!(
            "expected `{}`, got `{}`",
            types.format(self.session, ty1),
            types.format(self.session, ty2),
        );
        let secondary_label =
            format!("`{}` expected because of this", types.format(self.session, ty1));
        Diagnostic::error("`if` and `else` have incompatible types")
            .cross_source_annotations(loc2, primary_label, loc1, secondary_label)
            .emit(self.session);
    }

    pub fn emit_arg_count_mismatch(
        &mut self,
        expected: usize,
        actual: usize,
        call_loc: SrcLoc,
        param_def_loc: SrcLoc,
    ) {
        let call_label = format!("expected {}, got {actual}", fmt_count(expected, "argument"));
        let def_label = format!("defined with {}", fmt_count(expected, "parameter"));
        Diagnostic::error("wrong number of arguments")
            .cross_source_annotations(call_loc, call_label, param_def_loc, def_label)
            .emit(self.session);
    }

    pub fn emit_call_target_not_comptime(&mut self, call_loc: SrcLoc) {
        Diagnostic::error("call target must be known at compile time")
            .primary(call_loc.source, call_loc.span, "not known at compile time")
            .note("function calls are statically dispatched")
            .emit(self.session);
    }

    pub fn emit_closure_capture_not_comptime(&mut self, use_loc: SrcLoc, def_loc: SrcLoc) {
        Diagnostic::error("closure capture must be known at compile time")
            .cross_source_annotations(use_loc, "capture of runtime value", def_loc, "defined here")
            .note("closures can only capture values known at compile time")
            .emit(self.session);
    }

    pub fn emit_type_not_comptime(&mut self, loc: SrcLoc) {
        Diagnostic::error("type must be known at compile time")
            .primary(loc.source, loc.span, "not known at compile time")
            .emit(self.session);
    }

    pub fn emit_struct_type_index_not_comptime(&mut self, loc: SrcLoc) {
        Diagnostic::error("struct definition requires compile-time values")
            .primary(loc.source, loc.span, "type index is not known at compile time")
            .emit(self.session);
    }

    pub fn emit_runtime_ref_in_comptime(
        &mut self,
        source: SourceId,
        expr_span: SourceSpan,
        runtime_def: SourceSpan,
    ) {
        Diagnostic::error("runtime reference in comptime context")
            .element(
                Annotations::new(source)
                    .primary(expr_span, "expression with runtime reference")
                    .secondary(runtime_def, "runtime value defined here"),
            )
            .note("comptime contexts can only reference values known at compile time")
            .emit(self.session);
    }

    pub fn emit_runtime_eval_in_comptime(&mut self, expr: SrcLoc) {
        Diagnostic::error("attempting to evaluate runtime expression in comptime context")
            .primary(expr.source, expr.span, "runtime expression")
            .emit(self.session);
    }

    pub fn emit_entry_point_missing_terminator(&mut self, loc: SrcLoc) {
        Diagnostic::error("entry point must end with explicit terminator")
            .primary(loc.source, loc.span, "execution may reach end of entry point")
            .help(format!(
                "entry points must end with a terminating `never` expression (e.g. `{}()`, `{}(...)`, `{}()`)",
                builtin_names::STOP,
                builtin_names::REVERT,
                builtin_names::INVALID
            ))
            .emit(self.session);
    }

    pub fn emit_const_cycle(&mut self, name: StrId, loc: SrcLoc) {
        Diagnostic::error("cycle in constant evaluation")
            .primary(
                loc.source,
                loc.span,
                format!("`{}` depends on itself", self.session.lookup_name(name)),
            )
            .emit(self.session);
    }

    pub fn emit_not_yet_implemented(&mut self, functionality: &str, loc: SrcLoc) {
        Diagnostic::error(format!("{functionality} not yet implemented"))
            .element(Annotations::new(loc.source).no_label(loc.span, AnnotationKind::Primary))
            .emit(self.session);
    }

    pub fn emit_comptime_only_value_at_runtime(&mut self, use_loc: SrcLoc) {
        Diagnostic::error("use of comptime only value at runtime")
            .primary(use_loc.source, use_loc.span, "reference to comptime only value")
            .info("`let mut` definitions and mutable assignments require runtime-compatible values")
            .emit(self.session);
    }

    pub fn emit_mixed_comptime_runtime_struct(
        &mut self,
        source: SourceId,
        struct_lit_span: SourceSpan,
        comptime_only_field: hir::FieldInfo,
        runtime_only_field: hir::FieldInfo,
    ) {
        let (comptime_only_field_name, comptime_only_span) = self
            .session
            .lookup_name_spanned(comptime_only_field.name, comptime_only_field.name_offset);
        let (runtime_field_name, runtime_span) = self
            .session
            .lookup_name_spanned(runtime_only_field.name, runtime_only_field.name_offset);
        Diagnostic::error("mixing comptime and runtime data in struct")
            .element(
                Annotations::new(source)
                    .primary(struct_lit_span, "mixed struct literal")
                    .secondary(
                        comptime_only_span,
                        format!("`{comptime_only_field_name}` is comptime only"),
                    )
                    .secondary(runtime_span, format!("`{runtime_field_name}` not comptime known")),
            )
            .emit(self.session);
    }

    pub fn emit_no_matching_builtin_signature(
        &mut self,
        types: &TypeInterner,
        builtin: EvmBuiltin,
        arg_types: &[TypeId],
        loc: SrcLoc,
    ) {
        use std::fmt::Write;

        let mut note = format!("`{builtin}` accepts ");
        for (i, &sig) in builtin.signatures().iter().enumerate() {
            if i > 0 {
                note.push_str(", ");
            }
            note.push('(');
            for (j, &ty) in sig.inputs.iter().enumerate() {
                if j > 0 {
                    note.push_str(", ");
                }
                let _ = write!(note, "{}", types.format(self.session, ty));
            }
            note.push(')');
        }

        let (title, label) = if builtin.arg_count() == arg_types.len() {
            let mut args_str = String::new();
            for (i, &ty) in arg_types.iter().enumerate() {
                if i > 0 {
                    args_str.push_str(", ");
                }
                let _ = write!(args_str, "{}", types.format(self.session, ty));
            }
            (
                "no valid match for builtin signature",
                format!("`{builtin}` cannot be called with ({args_str})"),
            )
        } else {
            let expected = builtin.arg_count();
            (
                "wrong number of arguments",
                format!(
                    "`{builtin}` called with {}, but requires {}",
                    fmt_count(arg_types.len(), "argument"),
                    expected,
                ),
            )
        };

        Diagnostic::error(title).primary(loc.source, loc.span, label).note(note).emit(self.session);
    }

    pub fn emit_unsupported_eval_of_evm_builtin(&mut self, builtin: EvmBuiltin, loc: SrcLoc) {
        Diagnostic::error("builtin not supported at compile time")
            .primary(
                loc.source,
                loc.span,
                format!("`{}` cannot be evaluated at compile time", builtin.name()),
            )
            .emit(self.session);
    }

    pub fn emit_struct_lit_unexpected_field(
        &mut self,
        types: &TypeInterner,
        struct_ty: TypeId,
        lit_loc: SrcLoc,
        field: hir::FieldInfo,
    ) {
        let (field, field_span) = self.session.lookup_name_spanned(field.name, field.name_offset);
        Diagnostic::error("unexpected field")
            .primary(
                lit_loc.source,
                field_span,
                format!("`{}` has no field `{field}`", types.format(self.session, struct_ty)),
            )
            .emit(self.session);
    }

    pub fn emit_struct_unknown_field_access(
        &mut self,
        types: &TypeInterner,
        struct_ty: TypeId,
        expr_loc: SrcLoc,
        field_name: StrId,
    ) {
        Diagnostic::error("unknown field")
            .primary(
                expr_loc.source,
                expr_loc.span,
                format!(
                    "`{}` has no field `{}`",
                    types.format(self.session, struct_ty),
                    self.session.lookup_name(field_name),
                ),
            )
            .emit(self.session);
    }

    pub fn emit_struct_def_duplicate_field(
        &mut self,
        source: SourceId,
        str_name: StrId,
        first: SourceByteOffset,
        duplicate: SourceByteOffset,
    ) {
        let (name, first) = self.session.lookup_name_spanned(str_name, first);
        let (_, duplicate) = self.session.lookup_name_spanned(str_name, duplicate);
        Diagnostic::error("duplicate field name in struct definition")
            .element(
                Annotations::new(source)
                    .primary(duplicate, format!("`{name}` assigned more than once"))
                    .secondary(first, "first assigned here"),
            )
            .emit(self.session);
    }

    pub fn emit_struct_duplicate_field(
        &mut self,
        field_name: StrId,
        lit_loc: SrcLoc,
        first: SourceByteOffset,
        duplicate: SourceByteOffset,
    ) {
        let (field, first_span) = self.session.lookup_name_spanned(field_name, first);
        let (_, duplicate_span) = self.session.lookup_name_spanned(field_name, duplicate);

        Diagnostic::error("duplicate field")
            .cross_source_annotations(
                SrcLoc::new(lit_loc.source, duplicate_span),
                format!("`{field}` assigned more than once"),
                SrcLoc::new(lit_loc.source, first_span),
                "first assigned here",
            )
            .emit(self.session);
    }

    pub fn emit_struct_missing_field(
        &mut self,
        types: &TypeInterner,
        struct_ty: TypeId,
        field_name: StrId,
        lit_loc: SrcLoc,
    ) {
        Diagnostic::error("missing field")
            .primary(
                lit_loc.source,
                lit_loc.span,
                format!(
                    "missing field `{}` in `{}`",
                    self.session.lookup_name(field_name),
                    types.format(self.session, struct_ty),
                ),
            )
            .emit(self.session);
    }

    pub fn emit_runtime_call_with_recursion(&mut self, call_loc: SrcLoc) {
        Diagnostic::error("runtime recursion not supported")
            .primary(call_loc.source, call_loc.span, "runtime call that recurses")
            .note(
                "recursion is only allowed at compile time to ensure consistent\
 performance and iteration bounds",
            )
            .emit(self.session);
    }
}
