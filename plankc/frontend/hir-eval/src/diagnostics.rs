use alloy_primitives::U256;
use plank_core::{Span, must_use::MustUseStrict};
use plank_hir::{self as hir, operators::BinaryOp};
use plank_session::{Builtin, builtins::builtin_names, diagnostic::fmt_count, *};
use plank_values::{
    Compound, StructRef, TupleRef, Type, TypeFlags, TypeId, TypeInterner, ValueId, ValueInterner,
    builtins as builtin_sigs,
};

pub(crate) struct BindingLoc {
    pub r#use: SrcLoc,
    pub def: Option<SrcLoc>,
}

impl BindingLoc {
    pub fn inline(r#use: SrcLoc) -> Self {
        Self { r#use, def: None }
    }

    pub fn with_def(r#use: SrcLoc, def: SrcLoc) -> Self {
        Self { r#use, def: Some(def) }
    }
}

pub(crate) struct DiagCtx<'a> {
    pub session: &'a mut Session,
    pub types: &'a TypeInterner,
    preamble_call_site: Option<SrcLoc>,
}

#[must_use = "Must return to `DiagCtx` via `restore_preamble_call_site`, will panic if left unused"]
pub(crate) struct DiagCallSiteRestoreObligation {
    prev: Option<SrcLoc>,
    must_use: MustUseStrict,
}

impl<'a> DiagCtx<'a> {
    pub fn new(session: &'a mut Session, types: &'a TypeInterner) -> Self {
        Self { session, types, preamble_call_site: None }
    }

    pub fn set_preamble_call_site(&mut self, call_site: SrcLoc) -> DiagCallSiteRestoreObligation {
        DiagCallSiteRestoreObligation {
            prev: self.preamble_call_site.replace(call_site),
            must_use: MustUseStrict,
        }
    }

    pub fn restore_preamble_call_site(&mut self, restore: DiagCallSiteRestoreObligation) {
        let DiagCallSiteRestoreObligation { prev, must_use } = restore;
        self.preamble_call_site = prev;
        must_use.unchecked_destroy();
    }
}

impl DiagEmitter for DiagCtx<'_> {
    fn emit_diagnostic(&mut self, mut diagnostic: Diagnostic) {
        if let Some(call_site) = self.preamble_call_site {
            diagnostic = diagnostic.claim(
                Claim::new(Level::Note, "called here").element(
                    Annotations::new(call_site.source)
                        .no_label(call_site.span, AnnotationKind::Primary),
                ),
            );
        }
        self.session.emit_diagnostic(diagnostic);
    }
}

impl DiagCtx<'_> {
    fn format_expected_types(
        &self,
        values: &ValueInterner,
        expected_ty: TypeId,
        actual_ty: TypeId,
    ) -> (impl FnOnce(Diagnostic) -> Diagnostic, String, String) {
        self.format_type_mismatch("ed", values, expected_ty, actual_ty)
    }

    fn format_expects_types(
        &self,
        values: &ValueInterner,
        expected_ty: TypeId,
        actual_ty: TypeId,
    ) -> (impl FnOnce(Diagnostic) -> Diagnostic, String, String) {
        self.format_type_mismatch("s", values, expected_ty, actual_ty)
    }

    fn format_type_mismatch(
        &self,
        expect_suffix: &str,
        values: &ValueInterner,
        expected_ty: TypeId,
        actual_ty: TypeId,
    ) -> (impl FnOnce(Diagnostic) -> Diagnostic, String, String) {
        let expected = self.types.format(self.session, values, expected_ty).to_string();
        let actual = self.types.format(self.session, values, actual_ty).to_string();
        let repr_eq = expected == actual;
        let diff = if repr_eq { " different" } else { "" };
        let msg = format!("expect{expect_suffix} `{expected}`, got{diff} `{actual}`");

        (
            move |mut diag| {
                if repr_eq {
                    diag = diag.note("types appear identical because they contain types with the same name defined in different files");
                }
                if expected_ty == TypeId::VOID {
                    diag = diag.note("`void` is an alias for `tuple {}`")
                }
                diag
            },
            msg,
            expected,
        )
    }

    pub fn emit_type_mismatch(
        &mut self,
        values: &ValueInterner,
        expected_ty: TypeId,
        expected_loc: SrcLoc,
        actual_ty: TypeId,
        actual_loc: SrcLoc,
        add_called_here: bool,
    ) {
        let (maybe_add_note, primary_label, expected) =
            self.format_expected_types(values, expected_ty, actual_ty);
        let secondary_label = format!("`{expected}` expected because of this");
        let diagnostic = Diagnostic::error("mismatched types").cross_source_annotations(
            actual_loc,
            primary_label,
            expected_loc,
            secondary_label,
        );
        if add_called_here {
            maybe_add_note(diagnostic).emit(self)
        } else {
            maybe_add_note(diagnostic).emit(self.session);
        }
    }

    pub fn emit_type_not_type(&mut self, values: &ValueInterner, ty: TypeId, loc: BindingLoc) {
        let primary_label = format!(
            "expected {}, got value of type `{}`",
            builtin_names::TYPE,
            self.types.format(self.session, values, ty),
        );
        let diag = Diagnostic::error("value used as type");
        let diag = match loc.def {
            None => diag.primary(loc.r#use.source, loc.r#use.span, primary_label),
            Some(def) => {
                diag.cross_source_annotations(loc.r#use, primary_label, def, "defined here")
            }
        };
        diag.emit(self);
    }

    pub fn emit_struct_literal_field_type_mismatch(
        &mut self,
        values: &ValueInterner,
        expected_ty: TypeId,
        actual_ty: TypeId,
        field_value_loc: SrcLoc,
        field_name: StrId,
    ) {
        let name = self.session.lookup_name(field_name);
        let (maybe_add_note, primary, _) =
            self.format_expects_types(values, expected_ty, actual_ty);
        let diagnostic = Diagnostic::error("incorrect type for struct field").primary(
            field_value_loc.source,
            field_value_loc.span,
            format!("field `{name}` {primary}"),
        );
        maybe_add_note(diagnostic).emit(self);
    }

    pub fn emit_type_mismatch_simple(
        &mut self,
        values: &ValueInterner,
        expected_ty: TypeId,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        let (maybe_add_note, primary, _) =
            self.format_expected_types(values, expected_ty, actual_ty);
        maybe_add_note(
            Diagnostic::error("mismatched types").primary(loc.source, loc.span, primary),
        )
        .emit(self);
    }

    pub fn emit_not_a_struct_type(&mut self, values: &ValueInterner, ty: TypeId, loc: BindingLoc) {
        let primary_label =
            format!("`{}` is not a struct type", self.types.format(self.session, values, ty));
        let diag = Diagnostic::error("expected struct type");
        let diag = match loc.def {
            None => diag.primary(loc.r#use.source, loc.r#use.span, primary_label),
            Some(def) => {
                diag.cross_source_annotations(loc.r#use, primary_label, def, "defined here")
            }
        };
        diag.emit(self);
    }

    pub fn emit_member_on_non_struct(
        &mut self,
        values: &ValueInterner,
        ty: TypeId,
        loc: BindingLoc,
    ) {
        let primary_label = format!(
            "value of type `{}` is not a struct type",
            self.types.format(self.session, values, ty)
        );
        let diag = Diagnostic::error("no fields on type");
        let diag = match loc.def {
            None => diag.primary(loc.r#use.source, loc.r#use.span, primary_label),
            Some(def) => {
                diag.cross_source_annotations(loc.r#use, primary_label, def, "defined here")
            }
        };
        diag.emit(self);
    }

    pub fn emit_cbytes_unknown_attribute(&mut self, member: StrId, loc: SrcLoc) {
        Diagnostic::error("unknown cbytes attribute")
            .primary(
                loc.source,
                loc.span,
                format!("`cbytes` has no attribute `{}`", self.session.lookup_name(member)),
            )
            .help(format!("available attribute: `.{}`", builtin_names::LENGTH))
            .emit(self);
    }

    pub fn emit_not_callable(&mut self, values: &ValueInterner, ty: TypeId, loc: BindingLoc) {
        let primary_label =
            format!("`{}` is not callable", self.types.format(self.session, values, ty));
        let diag = Diagnostic::error("expected function");
        let diag = match loc.def {
            None => diag.primary(loc.r#use.source, loc.r#use.span, primary_label),
            Some(def) => {
                diag.cross_source_annotations(loc.r#use, primary_label, def, "defined here")
            }
        };
        diag.emit(self);
    }

    pub fn emit_incompatible_branch_types(
        &mut self,
        values: &ValueInterner,
        ty1: TypeId,
        loc1: SrcLoc,
        ty2: TypeId,
        loc2: SrcLoc,
    ) {
        let (maybe_add_note, primary_label, expected) =
            self.format_expected_types(values, ty1, ty2);
        let secondary_label = format!("`{expected}` expected because of this");
        let diagnostic = Diagnostic::error("`if` and `else` have incompatible types")
            .cross_source_annotations(loc2, primary_label, loc1, secondary_label);
        maybe_add_note(diagnostic).emit(self);
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
            .emit(self);
    }

    pub fn emit_call_target_not_comptime(&mut self, call_loc: SrcLoc) {
        Diagnostic::error("call target must be known at compile time")
            .primary(call_loc.source, call_loc.span, "not known at compile time")
            .note("function calls are statically dispatched")
            .emit(self);
    }

    pub fn emit_closure_capture_not_comptime(&mut self, use_loc: SrcLoc, def_loc: SrcLoc) {
        Diagnostic::error("closure capture must be known at compile time")
            .cross_source_annotations(use_loc, "capture of runtime value", def_loc, "defined here")
            .note("closures can only capture values known at compile time")
            .emit(self);
    }

    pub fn emit_type_not_comptime(&mut self, loc: SrcLoc) {
        Diagnostic::error("type must be known at compile time")
            .primary(loc.source, loc.span, "not known at compile time")
            .emit(self);
    }

    pub fn emit_struct_type_index_not_comptime(&mut self, loc: SrcLoc) {
        Diagnostic::error("struct definition requires compile-time values")
            .primary(loc.source, loc.span, "type index is not known at compile time")
            .emit(self);
    }

    pub fn emit_runtime_ref_in_comptime(&mut self, expr_loc: SrcLoc, runtime_def_loc: SrcLoc) {
        Diagnostic::error("runtime reference in comptime context")
            .cross_source_annotations(
                expr_loc,
                "expression with runtime reference",
                runtime_def_loc,
                "runtime value defined here",
            )
            .note("comptime contexts can only reference values known at compile time")
            .emit(self.session);
    }

    pub fn emit_runtime_eval_in_comptime(&mut self, expr: SrcLoc) {
        Diagnostic::error("attempting to evaluate runtime expression in comptime context")
            .primary(expr.source, expr.span, "runtime expression")
            .emit(self);
    }

    pub fn emit_eval_branch_quota_too_large(&mut self, loc: SrcLoc) {
        Diagnostic::error("eval branch quota is too large")
            .primary(loc.source, loc.span, "quota must fit in u32")
            .note(format!("maximum supported quota is {}", u32::MAX))
            .emit(self);
    }

    pub fn emit_comptime_loop_branch_quota_exhausted(
        &mut self,
        loc: SrcLoc,
        limit: u32,
        eval_branch_quota_start_loc: SrcLoc,
    ) {
        self.emit_comptime_quota_exhausted(loc, limit, eval_branch_quota_start_loc, "loop")
    }

    pub fn emit_comptime_call_branch_quota_exhausted(
        &mut self,
        loc: SrcLoc,
        limit: u32,
        eval_branch_quota_start_loc: SrcLoc,
    ) {
        self.emit_comptime_quota_exhausted(loc, limit, eval_branch_quota_start_loc, "call")
    }

    fn emit_comptime_quota_exhausted(
        &mut self,
        loc: SrcLoc,
        limit: u32,
        eval_branch_quota_start_loc: SrcLoc,
        reason: &'static str,
    ) {
        Diagnostic::error("comptime branch quota exhausted")
            .primary(
                loc.source,
                loc.span,
                format!("evaluating this {reason} exceeded the comptime branch quota"),
            )
            .note(format!("current eval branch quota is {limit}"))
            .claim(
                Claim::new(Level::Note, "comptime evaluation began here").element(
                    Annotations::new(eval_branch_quota_start_loc.source)
                        .no_label(eval_branch_quota_start_loc.span, AnnotationKind::Primary),
                ),
            )
            .emit(self);
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
            .emit(self);
    }

    pub fn emit_const_cycle(&mut self, name: StrId, loc: SrcLoc) {
        Diagnostic::error("cycle in constant evaluation")
            .primary(
                loc.source,
                loc.span,
                format!("`{}` depends on itself", self.session.lookup_name(name)),
            )
            .emit(self);
    }

    pub fn emit_comptime_only_value_at_runtime(&mut self, use_loc: SrcLoc) {
        Diagnostic::error("use of comptime-only value at runtime")
            .primary(use_loc.source, use_loc.span, "reference to comptime-only value")
            .emit(self);
    }

    pub fn emit_comptime_only_value_in_runtime_branch(
        &mut self,
        source: SourceId,
        value_span: SourceSpan,
        condition_span: SourceSpan,
    ) {
        Diagnostic::error("comptime-only value depends on runtime control flow")
            .element(
                Annotations::new(source)
                    .primary(value_span, "comptime-only value")
                    .secondary(condition_span, "runtime condition here"),
            )
            .note("branches with runtime conditions must produce runtime-compatible values")
            .help("make the condition comptime-known to evaluate only the taken branch")
            .emit(self);
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
                        format!("`{comptime_only_field_name}` is comptime-only"),
                    )
                    .secondary(runtime_span, format!("`{runtime_field_name}` not comptime-known")),
            )
            .emit(self);
    }

    pub fn emit_mixed_tuple_type(&mut self, expr: SrcLoc, tuple: TupleRef, values: &ValueInterner) {
        let mut runtime_field = None;
        let mut comptime_field = None;
        for (i, &field) in self.types.lookup_tuple(tuple).fields.iter().enumerate() {
            let flags = self.types.lookup(field).flags();
            if flags.contains(TypeFlags::COMPTIME_ONLY) {
                comptime_field.get_or_insert((i, field));
            }
            if flags.contains(TypeFlags::RUNTIME_ONLY) {
                runtime_field.get_or_insert((i, field));
            }
        }
        let (runtime_pos, runtime_ty) =
            runtime_field.expect("mixed should have at least one runtime");
        let (comptime_pos, comptime_ty) =
            comptime_field.expect("mixed should have at least one comptime");
        Diagnostic::error("defining uninstantiable type")
            .primary(
                expr.source,
                expr.span,
                format!(
                    "type `{}` of field #{} is runtime only, while type `{}` of field #{} is comptime only",
                    self.types.format(self.session, values, runtime_ty),
                    runtime_pos,
                    self.types.format(self.session, values, comptime_ty),
                    comptime_pos
                ),
            )
            .emit(self);
    }

    pub fn emit_mixed_struct_type(
        &mut self,
        expr: SrcLoc,
        r#struct: StructRef,
        values: &ValueInterner,
    ) {
        let mut runtime_field = None;
        let mut comptime_field = None;
        for &field in self.types.lookup_struct(r#struct).fields {
            let flags = self.types.lookup(field.ty).flags();
            if flags.contains(TypeFlags::COMPTIME_ONLY) {
                comptime_field.get_or_insert(field);
            }
            if flags.contains(TypeFlags::RUNTIME_ONLY) {
                runtime_field.get_or_insert(field);
            }
        }
        let runtime = runtime_field.expect("mixed should have at least one runtime");
        let comptime = comptime_field.expect("mixed should have at least one comptime");
        Diagnostic::error("defining uninstantiable type")
            .element(
                Annotations::new(expr.source)
                    .no_label(expr.span, AnnotationKind::Primary)
                    .secondary(
                        runtime.def_span,
                        format!(
                            "type `{}` is runtime only",
                            self.types.format(self.session, values, runtime.ty),
                        ),
                    )
                    .secondary(
                        comptime.def_span,
                        format!(
                            "type `{}` is comptime only",
                            self.types.format(self.session, values, comptime.ty),
                        ),
                    ),
            )
            .emit(self);
    }

    pub fn emit_mixed_comptime_runtime_tuple(
        &mut self,
        source: SourceId,
        tuple_lit_span: SourceSpan,
        comptime_only_element: SourceSpan,
        runtime_element: SourceSpan,
    ) {
        Diagnostic::error("mixing comptime and runtime data in tuple")
            .element(
                Annotations::new(source)
                    .primary(tuple_lit_span, "mixed tuple literal")
                    .secondary(comptime_only_element, "tuple element is comptime-only")
                    .secondary(runtime_element, "tuple element not comptime-known"),
            )
            .emit(self);
    }

    pub fn emit_set_field_on_comptime_only(
        &mut self,
        values: &ValueInterner,
        ty: TypeId,
        value_loc: SrcLoc,
        info: Compound<'_>,
    ) {
        let diagnostic = Diagnostic::error("mixing comptime and runtime data in compound type");
        let is_comptime_only_msg =
            format!("`{}` is a comptime-only type", self.types.format(self.session, values, ty));
        match info {
            Compound::Struct(r#struct) => diagnostic
                .cross_source_annotations(
                    value_loc,
                    "this value is only known at runtime",
                    r#struct.def_loc,
                    is_comptime_only_msg,
                )
                .emit(self),
            Compound::Tuple(_) => diagnostic
                .primary(value_loc.source, value_loc.span, "this value is only know at runtime")
                .note(is_comptime_only_msg)
                .emit(self),
        }
    }

    fn format_signatures_note(&self, values: &ValueInterner, builtin: Builtin) -> Option<String> {
        use std::fmt::Write;

        let signatures = builtin_sigs::builtin_signatures(builtin);
        if signatures.is_empty() {
            return None;
        }

        let mut note = format!("`{}` accepts ", builtin.name());
        for (i, sig) in signatures.iter().enumerate() {
            if i > 0 {
                note.push_str(", ");
            }
            note.push('(');
            for (j, &ty) in sig.inputs.iter().enumerate() {
                if j > 0 {
                    note.push_str(", ");
                }
                let _ = write!(note, "{}", self.types.format(self.session, values, ty));
            }
            note.push(')');
        }
        Some(note)
    }

    pub fn emit_wrong_arg_count(
        &mut self,
        values: &ValueInterner,
        builtin: Builtin,
        actual: usize,
        loc: SrcLoc,
    ) {
        let name = builtin.name();
        let expected = builtin_sigs::arg_count(builtin);

        let mut diag = Diagnostic::error("wrong number of arguments").primary(
            loc.source,
            loc.span,
            format!(
                "`{name}` called with {}, but requires {expected}",
                fmt_count(actual, "argument"),
            ),
        );

        if let Some(note) = self.format_signatures_note(values, builtin) {
            diag = diag.note(note);
        }

        diag.emit(self);
    }

    pub fn emit_no_matching_builtin_signature(
        &mut self,
        values: &ValueInterner,
        builtin: Builtin,
        arg_types: &[TypeId],
        loc: SrcLoc,
    ) {
        use std::fmt::Write;

        if builtin_sigs::arg_count(builtin) != arg_types.len() {
            return self.emit_wrong_arg_count(values, builtin, arg_types.len(), loc);
        }

        let name = builtin.name();
        let mut args_str = String::new();
        for (i, &ty) in arg_types.iter().enumerate() {
            if i > 0 {
                args_str.push_str(", ");
            }
            let _ = write!(args_str, "{}", self.types.format(self.session, values, ty));
        }

        let mut diag = Diagnostic::error("no valid match for builtin signature").primary(
            loc.source,
            loc.span,
            format!("`{name}` cannot be called with ({args_str})"),
        );

        if let Some(note) = self.format_signatures_note(values, builtin) {
            diag = diag.note(note);
        }

        diag.emit(self);
    }

    pub fn emit_unsupported_eval_of_runtime_builtin(
        &mut self,
        builtin: RuntimeBuiltin,
        loc: SrcLoc,
    ) {
        Diagnostic::error("builtin not supported at compile time")
            .primary(
                loc.source,
                loc.span,
                format!("`{}` cannot be evaluated at compile time", builtin.name()),
            )
            .emit(self);
    }

    pub fn emit_builtin_requires_evm_version(
        &mut self,
        builtin: RuntimeBuiltin,
        active: plank_evm::EvmVersion,
        required: plank_evm::EvmVersion,
        loc: SrcLoc,
    ) {
        Diagnostic::error(format!(
            "builtin `{}` requires EVM version `{required}` or later",
            builtin.name()
        ))
        .primary(
            loc.source,
            loc.span,
            format!("not available in the active EVM version `{active}`"),
        )
        .note(format!("recompile with `--evm-version {required}` or later"))
        .emit(self);
    }

    pub fn emit_custom_comptime_error(&mut self, message: impl Into<String>, loc: SrcLoc) {
        Diagnostic::error(message)
            .primary(loc.source, loc.span, "custom compile error triggered here")
            .emit(self);
    }

    pub fn emit_data_offset_in_comptime(&mut self, loc: SrcLoc) {
        Diagnostic::error(format!(
            "builtin `{}` not supported at compile time",
            builtin_names::DATA_OFFSET
        ))
        .primary(
            loc.source,
            loc.span,
            "`@data_offset` produces a runtime-only value and cannot be evaluated at compile time",
        )
        .emit(self);
    }

    pub fn emit_struct_lit_unexpected_field(
        &mut self,
        values: &ValueInterner,
        struct_ty: TypeId,
        lit_loc: SrcLoc,
        field: hir::FieldInfo,
    ) {
        let (field, field_span) = self.session.lookup_name_spanned(field.name, field.name_offset);
        Diagnostic::error("unexpected field")
            .primary(
                lit_loc.source,
                field_span,
                format!(
                    "`{}` has no field `{field}`",
                    self.types.format(self.session, values, struct_ty)
                ),
            )
            .emit(self);
    }

    pub fn emit_struct_unknown_field_access(
        &mut self,
        values: &ValueInterner,
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
                    self.types.format(self.session, values, struct_ty),
                    self.session.lookup_name(field_name),
                ),
            )
            .emit(self);
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
            .emit(self);
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
            .emit(self);
    }

    pub fn emit_struct_missing_field(
        &mut self,
        values: &ValueInterner,
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
                    self.types.format(self.session, values, struct_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_expected_compound_type_arg(
        &mut self,
        values: &ValueInterner,
        builtin: Builtin,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        Diagnostic::error("unexpected type kind")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{builtin}` expects a struct or tuple type, got `{}`",
                    self.types.format(self.session, values, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_expected_struct_type_arg(
        &mut self,
        values: &ValueInterner,
        builtin: Builtin,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        Diagnostic::error("unexpected type kind")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{builtin}` expects a struct type, got `{}`",
                    self.types.format(self.session, values, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_expected_type_arg(
        &mut self,
        values: &ValueInterner,
        builtin: Builtin,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        Diagnostic::error("expected type argument")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{builtin}` expects a type argument, got a value of type `{}`",
                    self.types.format(self.session, values, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_concat_cbytes_expected_tuple(
        &mut self,
        values: &ValueInterner,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        Diagnostic::error("invalid cbytes concat argument")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{}` expects a tuple, got `{}`",
                    builtin_names::CONCAT_CBYTES,
                    self.types.format(self.session, values, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_concat_cbytes_invalid_element(
        &mut self,
        values: &ValueInterner,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        Diagnostic::error("invalid cbytes concat element")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{}` tuple elements must be `{}` or `{}`, got `{}`",
                    builtin_names::CONCAT_CBYTES,
                    builtin_names::U256,
                    builtin_names::CBYTES,
                    self.types.format(self.session, values, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_field_index_out_of_bounds(
        &mut self,
        builtin: Builtin,
        index: U256,
        field_count: usize,
        loc: SrcLoc,
    ) {
        Diagnostic::error("field index out of bounds")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{builtin}`: field index {index} is out of bounds for type with {}",
                    fmt_count(field_count, "field"),
                ),
            )
            .emit(self);
    }

    pub fn emit_invalid_field_selector_type(
        &mut self,
        values: &ValueInterner,
        builtin: Builtin,
        target_ty: TypeId,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        let expected = if target_ty.is_tuple() {
            format!("`{}`", builtin_names::U256)
        } else {
            format!("`{}` or `{}`", builtin_names::U256, builtin_names::CBYTES)
        };
        Diagnostic::error("invalid field selector")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{builtin}` field selector must be {expected}, got `{}`",
                    self.types.format(self.session, values, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_unknown_field_name_selector(
        &mut self,
        values: &ValueInterner,
        builtin: Builtin,
        struct_ty: TypeId,
        field_name_bytes: CBytes,
        loc: SrcLoc,
    ) {
        let mut field_name = String::new();
        write_bytes_literal(&mut field_name, self.session.lookup_bytes_slice(field_name_bytes))
            .expect("writing to string cannot fail");
        Diagnostic::error("unknown field")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{builtin}`: `{}` has no field named {field_name}",
                    self.types.format(self.session, values, struct_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_bytes_slice_out_of_bounds(
        &mut self,
        start: U256,
        end: U256,
        len: u32,
        loc: SrcLoc,
    ) {
        Diagnostic::error("bytes slice out of bounds")
            .primary(
                loc.source,
                loc.span,
                format!("requested range {start}..{end} of bytes with length {len}"),
            )
            .note("requires `start <= end` and `end <= bytes.length`")
            .emit(self);
    }

    pub fn emit_cbytes_read_offset_out_of_bounds(&mut self, offset: U256, len: usize, loc: SrcLoc) {
        Diagnostic::error("cbytes read offset out of bounds")
            .primary(
                loc.source,
                loc.span,
                format!("offset {offset} is outside `{}` with length {len}", builtin_names::CBYTES),
            )
            .note("offset must be within `0..=bytes.length`")
            .emit(self);
    }

    pub fn emit_expected_comptime_arg(&mut self, builtin: Builtin, arg_name: &str, loc: SrcLoc) {
        Diagnostic::error("expected comptime argument")
            .primary(
                loc.source,
                loc.span,
                format!("`{builtin}` requires {arg_name} to be known at comptime"),
            )
            .emit(self);
    }

    pub fn emit_runtime_call_with_recursion(&mut self, call_loc: SrcLoc) {
        Diagnostic::error("runtime recursion not supported")
            .primary(call_loc.source, call_loc.span, "runtime call that recurses")
            .note(concat!(
                "recursion is only allowed at compile time to ensure consistent performance and",
                " iteration bounds"
            ))
            .emit(self);
    }

    pub fn emit_comptime_only_return_with_runtime_arg(
        &mut self,
        arg_loc: SrcLoc,
        call_loc: SrcLoc,
    ) {
        Diagnostic::error("runtime argument to function with comptime-only return type")
            .cross_source_annotations(
                arg_loc,
                "runtime argument here",
                call_loc,
                "function called here",
            )
            .note(concat!(
                "functions with comptime-only return types require all arguments to be known at",
                " compile time"
            ))
            .emit(self);
    }

    pub fn emit_comptime_param_got_runtime(&mut self, arg_def_loc: SrcLoc, param_def_loc: SrcLoc) {
        Diagnostic::error("attempted to pass runtime value as comptime parameter")
            .cross_source_annotations(
                arg_def_loc,
                "runtime argument defined here",
                param_def_loc,
                "parameter defined as comptime here",
            )
            .claim(
                Claim::new(
                    Level::Help,
                    "you can force compile time evaluation with a `comptime` block",
                )
                .element({
                    let span = arg_def_loc.span;
                    Patches::new(arg_def_loc.source)
                        .patch(Span::new(span.start, span.start), "comptime { ")
                        .patch(Span::new(span.end, span.end), " }")
                })
                .note("this only works if the expression is not fundamentally runtime"),
            )
            .emit(self);
    }

    pub fn emit_infinite_comptime_recursion(&mut self, call: SrcLoc) {
        Diagnostic::error("infinite comptime recursion detected")
            .primary(call.source, call.span, "call that recurses with identical arguments")
            .emit(self.session);
    }

    pub fn emit_uninit_incompatible_type(
        &mut self,
        ty: TypeId,
        expr: SrcLoc,
        values: &ValueInterner,
    ) {
        use builtin_names::*;

        let diagnostic = match self.types.lookup(ty) {
            Type::Primitive(primitive) => {
                assert!(primitive.flags().contains(TypeFlags::UNINIT_INCOMPATIBLE));
                Diagnostic::error("cannot create uninitialized value").primary(
                    expr.source,
                    expr.span,
                    format!("type `{}` cannot be uninitialized", primitive.name()),
                )
            }
            Type::Compound(Compound::Struct(r#struct)) => {
                let field = r#struct
                    .fields
                    .iter()
                    .find(|field| {
                        let r#type = self.types.lookup(field.ty);
                        r#type.flags().contains(TypeFlags::UNINIT_INCOMPATIBLE)
                    })
                    .expect("struct with no fields not uninit incompatible");
                Diagnostic::error("struct contains field that cannot be uninitialized")
                    .cross_source_annotations(
                        expr,
                        format!("cannot use {} on this struct", builtin_names::UNINIT),
                        SrcLoc::new(r#struct.def_loc.source, field.def_span),
                        format!(
                            "type `{}` cannot be uninitialized",
                            self.types.format(self.session, values, field.ty)
                        ),
                    )
            }
            Type::Compound(Compound::Tuple(tuple)) => {
                let field_pos = tuple
                    .fields
                    .iter()
                    .position(|element| {
                        let r#type = self.types.lookup(*element);
                        r#type.flags().contains(TypeFlags::UNINIT_INCOMPATIBLE)
                    })
                    .expect("empty tuple not uninit incompatible");
                let element = tuple.fields[field_pos];
                Diagnostic::error("tuple contains field that cannot be uninitialized").primary(
                    expr.source,
                    expr.span,
                    format!(
                        "field {} of type `{}` cannot be uninitialized",
                        field_pos,
                        self.types.format(self.session, values, element)
                    ),
                )
            }
        };

        diagnostic
            .help(
                format!("{UNINIT} only supports types that do not contain {NEVER} or {FUNCTION}",),
            )
            .emit(self);
    }

    pub fn emit_uninit_memptr_in_comptime(&mut self, loc: SrcLoc) {
        Diagnostic::error(format!(
            "cannot use {} on memptr type at comptime",
            builtin_names::UNINIT
        ))
        .primary(loc.source, loc.span, "memptr requires runtime allocation")
        .emit(self);
    }

    pub fn emit_operator_not_supported(
        &mut self,
        values: &ValueInterner,
        op: impl std::fmt::Display,
        ty: TypeId,
        expr: SrcLoc,
    ) {
        Diagnostic::error("operator not supported")
            .primary(
                expr.source,
                expr.span,
                format!(
                    "operator '{op}' is not supported for type `{}`",
                    self.types.format(self.session, values, ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_operator_not_supported_for_memptr(
        &mut self,
        op: impl std::fmt::Display,
        expr: SrcLoc,
    ) {
        Diagnostic::error("operator not supported")
            .primary(
                expr.source,
                expr.span,
                format!("operator '{op}' is not supported for type `memptr`"),
            )
            .help("only wrapping operators `+%` and `-%` are supported for `memptr`")
            .emit(self);
    }

    pub fn emit_operator_type_mismatch(
        &mut self,
        values: &ValueInterner,
        lhs_ty: TypeId,
        rhs_ty: TypeId,
        loc: SrcLoc,
    ) {
        let (maybe_add_note, label, _) = self.format_expected_types(values, lhs_ty, rhs_ty);
        let diagnostic = Diagnostic::error("mismatched types").primary(loc.source, loc.span, label);
        maybe_add_note(diagnostic).emit(self);
    }

    pub fn emit_comptime_arithmetic_overflow(&mut self, op: impl std::fmt::Display, loc: SrcLoc) {
        Diagnostic::error("arithmetic overflow")
            .primary(loc.source, loc.span, format!("'{op}' overflow at compile time"))
            .emit(self);
    }

    pub fn emit_comptime_arithmetic_underflow(&mut self, op: impl std::fmt::Display, loc: SrcLoc) {
        Diagnostic::error("arithmetic underflow")
            .primary(loc.source, loc.span, format!("'{op}' underflow at compile time"))
            .emit(self);
    }

    pub fn emit_comptime_division_by_zero(&mut self, op: BinaryOp, expr: SrcLoc) {
        Diagnostic::error("division by zero")
            .primary(expr.source, expr.span, format!("'{op}' division by zero at compile time"))
            .info(concat!(
                "for EVM behavior where division by zero returns 0, use `@evm_div` or `@evm_sdiv`,",
                " note that the rounding direction may differ"
            ))
            .emit(self);
    }

    pub fn emit_comptime_modulo_by_zero(&mut self, op: BinaryOp, expr: SrcLoc) {
        Diagnostic::error("modulo by zero")
            .primary(expr.source, expr.span, format!("'{op}' modulo by zero at compile time"))
            .info("for EVM behavior where modulo by zero returns 0, use `@evm_mod`")
            .emit(self);
    }

    pub fn emit_std_operator_not_a_function(&mut self, name: &str, loc: SrcLoc) {
        Diagnostic::error("invalid standard library operator")
            .primary(loc.source, loc.span, format!("`{name}` is not a function"))
            .emit(self);
    }

    pub fn emit_failed_to_resolve_std_fn(&mut self, source: SourceId, op_name: &str) {
        Diagnostic::error(format!("failed to resolve core operation handler `{op_name}`"))
            .element(Element::Origin { path: source })
            .emit(self);
    }

    pub fn emit_found_compile_log(&mut self, first_loc: SrcLoc) {
        Diagnostic::error("found compile log statement")
            .element(
                Annotations::new(first_loc.source)
                    .no_label(first_loc.span, AnnotationKind::Primary),
            )
            .emit(self);
    }

    // TODO: Code smell due to https://github.com/plankevm/plank-monorepo/issues/253
    pub fn record_compile_log(&mut self, values: &ValueInterner, value_id: ValueId, loc: SrcLoc) {
        let msg = values.format_value(self.session, self.types, value_id).to_string();
        self.session.emit_compile_log(CompileLog { loc, msg });
    }
}
