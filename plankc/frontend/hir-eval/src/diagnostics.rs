use alloy_primitives::U256;
use plank_core::{Span, must_use::MustUseStrict};
use plank_hir::{self as hir, operators::BinaryOp};
use plank_session::{Builtin, builtins::builtin_names, diagnostic::fmt_count, *};
use plank_values::{PrimitiveType, TypeId, TypeInterner, builtins as builtin_sigs};

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
    pub fn emit_type_mismatch(
        &mut self,
        expected_ty: TypeId,
        expected_loc: SrcLoc,
        actual_ty: TypeId,
        actual_loc: SrcLoc,
        add_called_here: bool,
    ) {
        let primary_label = format!(
            "expected `{}`, got `{}`",
            self.types.format(self.session, expected_ty),
            self.types.format(self.session, actual_ty),
        );
        let secondary_label =
            format!("`{}` expected because of this", self.types.format(self.session, expected_ty));
        let diagnostic = Diagnostic::error("mismatched types").cross_source_annotations(
            actual_loc,
            primary_label,
            expected_loc,
            secondary_label,
        );
        if add_called_here {
            diagnostic.emit(self)
        } else {
            diagnostic.emit(self.session);
        }
    }

    pub fn emit_type_not_type(&mut self, ty: TypeId, loc: BindingLoc) {
        let primary_label = format!(
            "expected {}, got value of type `{}`",
            builtin_names::TYPE,
            self.types.format(self.session, ty),
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
                    self.types.format(self.session, expected_ty),
                    self.types.format(self.session, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_type_mismatch_simple(
        &mut self,
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
                    self.types.format(self.session, expected_ty),
                    self.types.format(self.session, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_not_a_struct_type(&mut self, ty: TypeId, loc: BindingLoc) {
        let primary_label =
            format!("`{}` is not a struct type", self.types.format(self.session, ty));
        let diag = Diagnostic::error("expected struct type");
        let diag = match loc.def {
            None => diag.primary(loc.r#use.source, loc.r#use.span, primary_label),
            Some(def) => {
                diag.cross_source_annotations(loc.r#use, primary_label, def, "defined here")
            }
        };
        diag.emit(self);
    }

    pub fn emit_member_on_non_struct(&mut self, ty: TypeId, loc: BindingLoc) {
        let primary_label =
            format!("value of type `{}` is not a struct type", self.types.format(self.session, ty));
        let diag = Diagnostic::error("no fields on type");
        let diag = match loc.def {
            None => diag.primary(loc.r#use.source, loc.r#use.span, primary_label),
            Some(def) => {
                diag.cross_source_annotations(loc.r#use, primary_label, def, "defined here")
            }
        };
        diag.emit(self);
    }

    pub fn emit_not_callable(&mut self, ty: TypeId, loc: BindingLoc) {
        let primary_label = format!("`{}` is not callable", self.types.format(self.session, ty));
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
        Diagnostic::error("`if` and `else` have incompatible types")
            .cross_source_annotations(loc2, primary_label, loc1, secondary_label)
            .emit(self);
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

    pub fn emit_not_yet_implemented(&mut self, functionality: &str, loc: SrcLoc) {
        Diagnostic::error(format!("{functionality} not yet implemented"))
            .element(Annotations::new(loc.source).no_label(loc.span, AnnotationKind::Primary))
            .emit(self);
    }

    pub fn emit_comptime_only_value_at_runtime(&mut self, use_loc: SrcLoc) {
        Diagnostic::error("use of comptime-only value at runtime")
            .primary(use_loc.source, use_loc.span, "reference to comptime-only value")
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

    pub fn emit_set_field_on_comptime_only_struct(
        &mut self,
        struct_ty: TypeId,
        value_loc: SrcLoc,
        struct_def_loc: SrcLoc,
    ) {
        let struct_name = self.types.format(self.session, struct_ty);
        Diagnostic::error("mixing comptime and runtime data in struct")
            .cross_source_annotations(
                value_loc,
                "this value is only known at runtime",
                struct_def_loc,
                format!("`{struct_name}` is comptime-only"),
            )
            .emit(self);
    }

    fn format_signatures_note(&self, builtin: Builtin) -> Option<String> {
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
                let _ = write!(note, "{}", self.types.format(self.session, ty));
            }
            note.push(')');
        }
        Some(note)
    }

    pub fn emit_wrong_arg_count(&mut self, builtin: Builtin, actual: usize, loc: SrcLoc) {
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

        if let Some(note) = self.format_signatures_note(builtin) {
            diag = diag.note(note);
        }

        diag.emit(self);
    }

    pub fn emit_no_matching_builtin_signature(
        &mut self,
        builtin: Builtin,
        arg_types: &[TypeId],
        loc: SrcLoc,
    ) {
        use std::fmt::Write;

        if builtin_sigs::arg_count(builtin) != arg_types.len() {
            return self.emit_wrong_arg_count(builtin, arg_types.len(), loc);
        }

        let name = builtin.name();
        let mut args_str = String::new();
        for (i, &ty) in arg_types.iter().enumerate() {
            if i > 0 {
                args_str.push_str(", ");
            }
            let _ = write!(args_str, "{}", self.types.format(self.session, ty));
        }

        let mut diag = Diagnostic::error("no valid match for builtin signature").primary(
            loc.source,
            loc.span,
            format!("`{name}` cannot be called with ({args_str})"),
        );

        if let Some(note) = self.format_signatures_note(builtin) {
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

    pub fn emit_struct_lit_unexpected_field(
        &mut self,
        struct_ty: TypeId,
        lit_loc: SrcLoc,
        field: hir::FieldInfo,
    ) {
        let (field, field_span) = self.session.lookup_name_spanned(field.name, field.name_offset);
        Diagnostic::error("unexpected field")
            .primary(
                lit_loc.source,
                field_span,
                format!("`{}` has no field `{field}`", self.types.format(self.session, struct_ty)),
            )
            .emit(self);
    }

    pub fn emit_struct_unknown_field_access(
        &mut self,
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
                    self.types.format(self.session, struct_ty),
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
                    self.types.format(self.session, struct_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_expected_struct_type_arg(
        &mut self,
        builtin: Builtin,
        actual_ty: TypeId,
        loc: SrcLoc,
    ) {
        Diagnostic::error("expected struct type")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{builtin}` expects a struct type, got `{}`",
                    self.types.format(self.session, actual_ty),
                ),
            )
            .emit(self);
    }

    pub fn emit_expected_type_arg(&mut self, builtin: Builtin, actual_ty: TypeId, loc: SrcLoc) {
        Diagnostic::error("expected type argument")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "`{builtin}` expects a type argument, got a value of type `{}`",
                    self.types.format(self.session, actual_ty),
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
                    "`{builtin}`: field index {index} is out of bounds for struct with {}",
                    fmt_count(field_count, "field"),
                ),
            )
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

    fn uninit_help() -> String {
        use builtin_names::*;
        format!(
            "{UNINIT} only supports {U256}, {BOOL}, {VOID}, {TYPE}, {MEMORY_POINTER} and struct types",
        )
    }

    pub fn emit_invalid_uninit_type(&mut self, ty: PrimitiveType, loc: SrcLoc) {
        Diagnostic::error("cannot create uninitialized value")
            .primary(loc.source, loc.span, format!("type '{}' cannot be uninitialized", ty.name()))
            .help(Self::uninit_help())
            .emit(self);
    }

    pub fn emit_invalid_uninit_struct_field(
        &mut self,
        ty: PrimitiveType,
        loc: SrcLoc,
        field_loc: SrcLoc,
    ) {
        Diagnostic::error("struct contains field that cannot be uninitialized")
            .primary(
                loc.source,
                loc.span,
                format!("cannot use {} on this struct", builtin_names::UNINIT),
            )
            .element(
                Annotations::new(field_loc.source).secondary(
                    field_loc.span,
                    format!("type '{}' cannot be uninitialized", ty.name()),
                ),
            )
            .help(Self::uninit_help())
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

    pub fn emit_never_as_struct_field(&mut self, field_def: SrcLoc, name: StrId) {
        let name = self.session.lookup_name(name);
        Diagnostic::error(format!("`{}` not valid struct field type", builtin_names::NEVER))
            .primary(
                field_def.source,
                field_def.span,
                format!("type of `{name}` evaluated to `{}`", builtin_names::NEVER),
            )
            .emit(self);
    }

    pub fn emit_operator_not_supported(
        &mut self,
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
                    self.types.format(self.session, ty),
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

    pub fn emit_operator_type_mismatch(&mut self, lhs_ty: TypeId, rhs_ty: TypeId, loc: SrcLoc) {
        Diagnostic::error("mismatched types")
            .primary(
                loc.source,
                loc.span,
                format!(
                    "expected `{}`, got `{}`",
                    self.types.format(self.session, lhs_ty),
                    self.types.format(self.session, rhs_ty),
                ),
            )
            .emit(self);
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
}
