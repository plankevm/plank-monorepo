use alloy_primitives::U256;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{Builtin, MaybePoisoned, RuntimeBuiltin, SourceSpan, builtins::BuiltinKind};
use plank_values::{
    Field, PrimitiveType, StructView, Type, TypeId, TypeInterner, Value, ValueId, ValueInterner,
    builtins as builtin_sigs,
};

use crate::{
    diagnostics::DiagCtx,
    scope::{Diverge, EvalValue, LocalState, Scope},
};
use plank_session::{Poisoned, SrcLoc};

impl<'a, 'ctx> Scope<'a, 'ctx> {
    pub(crate) fn eval_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        match builtin {
            Builtin::Runtime(runtime) => {
                if runtime.foldable() {
                    self.eval_runtime_foldable_builtin(runtime, args, expr_span)
                } else {
                    self.eval_runtime_only_builtin(runtime, args, expr_span)
                }
            }
            builtin => match builtin.kind() {
                BuiltinKind::Comptime => self.eval_comptime_builtin(builtin, args, expr_span),
                BuiltinKind::ComptimeDynamic { .. } => {
                    self.eval_comptime_dynamic_builtin(builtin, args, expr_span)
                }
                BuiltinKind::RuntimeFoldable | BuiltinKind::RuntimeOnly => {
                    unreachable!("already matched")
                }
            },
        }
    }

    fn eval_runtime_foldable_builtin(
        &mut self,
        builtin: RuntimeBuiltin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let result_type = self.resolve_runtime_builtin_result_type(builtin, args, expr_span)?;

        let hir_args = &self.hir.call_args[args];
        let folded = self.with_values_buf(|this, values_buf_offset| {
            for &arg in hir_args {
                let (state, _arg_use_span, arg_origin) =
                    this.bindings[arg].poisoned().expect("invariant: arg type check checks poison");
                match state {
                    LocalState::Comptime(vid) => this.values_buf.push(vid),
                    LocalState::Runtime(_) if this.is_comptime() => {
                        this.diag_ctx.emit_runtime_ref_in_comptime(
                            this.loc(expr_span),
                            this.origin_loc(arg_origin),
                        );
                        return Err(Poisoned);
                    }
                    LocalState::Runtime(_) => return Ok(None),
                }
            }
            Ok(Some(fold_runtime_builtin(
                builtin,
                &this.eval.values_buf[values_buf_offset..],
                this.eval.values,
            )))
        })?;
        if let Some(value) = folded {
            return Ok(Ok(EvalValue::Comptime(value)));
        }

        Ok(self.emit_runtime_builtin_mir(builtin, args, result_type))
    }

    fn eval_runtime_only_builtin(
        &mut self,
        builtin: RuntimeBuiltin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let result_type = self.resolve_runtime_builtin_result_type(builtin, args, expr_span)?;

        if self.is_comptime() {
            self.diag_ctx.emit_unsupported_eval_of_runtime_builtin(builtin, self.loc(expr_span));
            if result_type == TypeId::NEVER {
                return Ok(Err(Diverge::ControlFlowPoisoned));
            } else {
                return Err(Poisoned);
            }
        }

        Ok(self.emit_runtime_builtin_mir(builtin, args, result_type))
    }

    fn resolve_runtime_builtin_result_type(
        &mut self,
        builtin: RuntimeBuiltin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<TypeId> {
        let hir_args = &self.hir.call_args[args];
        let expr_loc = self.loc(expr_span);
        self.with_types_buf(|this, types_buf_offset| {
            for &arg in hir_args {
                let ty = this.state_type(this.bindings[arg].state?);
                this.eval.types_buf.push(ty);
            }

            let arg_types = &this.eval.types_buf[types_buf_offset..];
            builtin_sigs::resolve_result_type(builtin.into(), arg_types).ok_or_else(|| {
                this.diag_ctx.emit_no_matching_builtin_signature(
                    builtin.into(),
                    &this.eval.types_buf[types_buf_offset..],
                    expr_loc,
                );
                Poisoned
            })
        })
    }

    fn emit_runtime_builtin_mir(
        &mut self,
        builtin: RuntimeBuiltin,
        args: hir::CallArgsId,
        result_type: TypeId,
    ) -> Result<EvalValue, Diverge> {
        let hir_args = &self.hir.call_args[args];
        let mir_args = self.with_locals_buf(|this, locals_buf_offset| {
            for &arg in hir_args {
                let state =
                    this.bindings[arg].state.expect("invariant: arg type check checks poison");
                if let LocalState::Comptime(vid) = state {
                    assert!(
                        !this.is_comptime_only(vid),
                        "runtime builtin typechecks for comptime only value"
                    );
                }
                let ty = this.state_type(state);
                let local = this.materialize_as_local(state, ty);
                this.locals_buf.push(local);
            }
            this.eval.mir_args.push_copy_slice(&this.eval.locals_buf[locals_buf_offset..])
        });

        let expr = mir::Expr::RuntimeBuiltinCall { builtin, args: mir_args };
        if result_type == TypeId::NEVER {
            // We diverge after this so we need to make sure the call is actually included.
            let target = self.mir_types.push(result_type);
            self.emit(mir::Instruction::Set { target, expr });
            return Err(Diverge::BlockEnd(None));
        }

        Ok(EvalValue::Runtime { expr, result_type })
    }

    fn eval_comptime_builtin(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let hir_args = &self.hir.call_args[args];
        let expr_loc = self.loc(expr_span);

        if builtin_sigs::arg_count(builtin) != hir_args.len() {
            self.diag_ctx.emit_wrong_arg_count(builtin, hir_args.len(), expr_loc);
            return Err(Poisoned);
        }

        match builtin {
            Builtin::IsStruct => {
                let &[ty_local] = hir_args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
                let is_struct = !ty.is_primitive();
                Ok(Ok(EvalValue::Comptime(is_struct.into())))
            }
            Builtin::FieldCount => {
                let &[r#struct] = hir_args else { unreachable!("arg count checked") };
                let ty = self.expect_type_arg(r#struct, builtin, expr_span)?;
                let r#struct = self.expect_struct_type(ty, builtin, expr_span)?;
                let count = U256::from(r#struct.fields.len());
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(count))))
            }
            _ => unreachable!("not a comptime builtin: {builtin}"),
        }
    }

    fn eval_comptime_dynamic_builtin(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let hir_args = &self.hir.call_args[args];
        let expr_loc = self.loc(expr_span);

        if builtin_sigs::arg_count(builtin) != hir_args.len() {
            self.diag_ctx.emit_wrong_arg_count(builtin, hir_args.len(), expr_loc);
            return Err(Poisoned);
        }

        match builtin {
            Builtin::FieldType => self.eval_field_type(hir_args, builtin, expr_span),
            Builtin::GetField => self.eval_get_field(hir_args, builtin, expr_span),
            Builtin::SetField => self.eval_set_field(hir_args, builtin, expr_span),
            Builtin::Uninit => self.eval_uninit(hir_args, builtin, expr_span),
            _ => unreachable!("not a comptime dynamic builtin: {builtin}"),
        }
    }

    fn eval_field_type(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[ty, field_index] = args else { unreachable!("arg count checked") };
        let ty = self.expect_type_arg(ty, builtin, expr_span)?;
        let (_struct, field, _index) =
            self.resolve_struct_field_index(ty, field_index, builtin, expr_span)?;
        Ok(Ok(EvalValue::Comptime(self.eval.values.intern_type(field.ty))))
    }

    fn eval_get_field(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[r#struct, field_index] = args else { unreachable!("arg count checked") };
        let instance_state = self.bindings[r#struct].state?;
        let ty = self.state_type(instance_state);
        let (_struct, field, field_index) =
            self.resolve_struct_field_index(ty, field_index, builtin, expr_span)?;

        match instance_state {
            LocalState::Comptime(vid) => match self.values.lookup(vid) {
                Value::StructVal { fields, .. } => {
                    Ok(Ok(EvalValue::Comptime(fields[field_index as usize])))
                }
                _ => unreachable!("invariant: type checked as struct"),
            },
            LocalState::Runtime(local) => Ok(Ok(EvalValue::Runtime {
                expr: mir::Expr::FieldAccess { object: local, field_index },
                result_type: field.ty,
            })),
        }
    }

    fn eval_set_field(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[instance, field_index, field_value] = args else { unreachable!("arg count checked") };
        let instance_state = self.bindings[instance].state?;
        let instance_ty = self.state_type(instance_state);
        let (r#struct, field, field_index) =
            self.resolve_struct_field_index(instance_ty, field_index, builtin, expr_span)?;

        let new_value_state = self.bindings[field_value].state?;
        let expected_field_type = field.ty;
        let actual_ty = self.state_type(new_value_state);
        if !actual_ty.is_assignable_to(expected_field_type) {
            let field_def_loc = self.loc(field.def_span);
            self.diag_ctx.emit_type_mismatch(
                expected_field_type,
                field_def_loc,
                actual_ty,
                self.loc(self.bindings[field_value].use_span),
                false,
            );
            return Err(Poisoned);
        }

        // Both comptime: pure comptime fold.
        if let (LocalState::Comptime(instance_vid), LocalState::Comptime(new_value_vid)) =
            (instance_state, new_value_state)
        {
            return Ok(self.with_values_buf(|this, values_buf_offset| {
                match this.eval.values.lookup(instance_vid) {
                    Value::StructVal { fields: old_fields, .. } => {
                        this.eval.values_buf.extend_from_slice(old_fields);
                    }
                    _ => unreachable!("invariant: type checked as struct"),
                }
                let fields = &mut this.eval.values_buf[values_buf_offset..];
                fields[field_index as usize] = new_value_vid;
                Ok(EvalValue::Comptime(
                    this.eval.values.intern(Value::StructVal { ty: instance_ty, fields }),
                ))
            }));
        }

        // At least one side is runtime: emit MIR.
        if self.eval.types.is_comptime_only(instance_ty) {
            self.diag_ctx.emit_set_field_on_comptime_only_struct(
                instance_ty,
                self.loc(self.bindings[field_value].use_span),
                r#struct.def_loc,
            );
            return Err(Poisoned);
        }

        let instance_local = self.materialize_as_local(instance_state, instance_ty);
        let mir_fields = self.with_locals_buf(|this, locals_buf_offset| {
            for (cur_field_idx, &field) in (0..).zip(r#struct.fields) {
                if cur_field_idx == field_index {
                    let local = this.materialize_as_local(new_value_state, expected_field_type);
                    this.locals_buf.push(local);
                    continue;
                }
                let target = this.mir_types.push(field.ty);
                this.emit(mir::Instruction::Set {
                    target,
                    expr: mir::Expr::FieldAccess {
                        object: instance_local,
                        field_index: cur_field_idx,
                    },
                });
                this.locals_buf.push(target);
            }
            this.eval.mir_args.push_copy_slice(&this.eval.locals_buf[locals_buf_offset..])
        });

        Ok(Ok(EvalValue::Runtime {
            expr: mir::Expr::StructLit { ty: instance_ty, fields: mir_fields },
            result_type: instance_ty,
        }))
    }

    fn eval_uninit(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let &[ty_local] = args else { unreachable!("arg count checked") };
        let ty = self.expect_type_arg(ty_local, builtin, expr_span)?;
        if validate_uninit_type(ty, self.eval.types, self.diag_ctx, self.loc(expr_span), None) {
            return Err(Poisoned);
        }

        // Types that require runtime allocation (memptr, structs containing memptr)
        // produce MIR directly.
        if contains_memptr(ty, self.eval.types) {
            if self.is_comptime() {
                self.diag_ctx.emit_uninit_memptr_in_comptime(self.loc(expr_span));
                return Err(Poisoned);
            }
            return Ok(Ok(self.emit_uninit_runtime(ty)));
        }

        Ok(Ok(EvalValue::Comptime(build_uninit_comptime(
            ty,
            self.eval.types,
            self.eval.values,
            &mut self.eval.values_buf,
        ))))
    }

    /// Emits MIR instructions for a runtime uninit value (memptr or struct containing memptr).
    fn emit_uninit_runtime(&mut self, ty: TypeId) -> EvalValue {
        let local = self.emit_uninit_runtime_local(ty);
        EvalValue::Runtime { expr: mir::Expr::LocalRef(local), result_type: ty }
    }

    fn emit_uninit_runtime_local(&mut self, ty: TypeId) -> mir::LocalId {
        match ty.as_primitive() {
            Ok(PrimitiveType::U256) => {
                let target = self.mir_types.push(TypeId::U256);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(ValueId::ZERO) });
                target
            }
            Ok(PrimitiveType::Bool) => {
                let target = self.mir_types.push(TypeId::BOOL);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(ValueId::FALSE) });
                target
            }
            Ok(PrimitiveType::MemoryPointer) => {
                let size_local = self.mir_types.push(TypeId::U256);
                self.emit(mir::Instruction::Set {
                    target: size_local,
                    expr: mir::Expr::Const(ValueId::ZERO),
                });
                let args = self.eval.mir_args.push_copy_slice(&[size_local]);
                let target = self.mir_types.push(TypeId::MEMORY_POINTER);
                self.emit(mir::Instruction::Set {
                    target,
                    expr: mir::Expr::RuntimeBuiltinCall {
                        builtin: RuntimeBuiltin::DynamicAllocAnyBytes,
                        args,
                    },
                });
                target
            }
            Ok(PrimitiveType::Void) => {
                let target = self.mir_types.push(TypeId::VOID);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(ValueId::VOID) });
                target
            }
            Ok(PrimitiveType::Type | PrimitiveType::Function | PrimitiveType::Never) => {
                unreachable!("void/type/function/never do not produce runtime locals")
            }
            Err(struct_ref) => {
                let fields = self.with_locals_buf(|this, offset| {
                    let view = this.eval.types.lookup_struct(struct_ref);
                    for field in view.fields {
                        let local = this.emit_uninit_runtime_local(field.ty);
                        this.locals_buf.push(local);
                    }
                    this.eval.mir_args.push_copy_slice(&this.eval.locals_buf[offset..])
                });
                let struct_ty = TypeId::from_struct(struct_ref);
                let target = self.mir_types.push(struct_ty);
                self.emit(mir::Instruction::Set {
                    target,
                    expr: mir::Expr::StructLit { ty: struct_ty, fields },
                });
                target
            }
        }
    }

    fn resolve_struct_field_index(
        &mut self,
        ty: TypeId,
        index_arg: hir::LocalId,
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<(StructView<'a>, Field, u32)> {
        let r#struct = self.expect_struct_type(ty, builtin, expr_span)?;
        let index = self.expect_comptime_field_index(index_arg, builtin, expr_span)?;
        let field_and_index = u32::try_from(index).ok().and_then(|index| {
            let &field = r#struct.fields.get(index as usize)?;
            Some((field, index))
        });
        let Some((field, index)) = field_and_index else {
            self.diag_ctx.emit_field_index_out_of_bounds(
                builtin,
                index,
                r#struct.fields.len(),
                self.loc(self.bindings[index_arg].use_span),
            );
            return Err(Poisoned);
        };
        Ok((r#struct, field, index))
    }

    fn expect_type_arg(
        &mut self,
        arg_local: hir::LocalId,
        builtin: Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<TypeId> {
        let state = self.bindings[arg_local].state?;
        if let LocalState::Comptime(vid) = state
            && let Value::Type(ty) = self.values.lookup(vid)
        {
            return Ok(ty);
        }
        let actual_ty = self.state_type(state);
        self.diag_ctx.emit_expected_type_arg(builtin, actual_ty, self.loc(span));
        Err(Poisoned)
    }

    fn expect_comptime_field_index(
        &mut self,
        arg_local: hir::LocalId,
        builtin: Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<U256> {
        let arg_binding = self.bindings[arg_local];
        let state = arg_binding.state?;
        let LocalState::Comptime(vid) = state else {
            self.diag_ctx.emit_expected_comptime_arg(builtin, "field index", self.loc(span));
            return Err(Poisoned);
        };
        let Value::BigNum(n) = self.values.lookup(vid) else {
            self.diag_ctx.emit_type_mismatch_simple(
                TypeId::U256,
                self.eval.values.type_of_value(vid),
                self.loc(arg_binding.use_span),
            );
            return Err(Poisoned);
        };
        Ok(n)
    }

    fn expect_struct_type(
        &mut self,
        ty: TypeId,
        builtin: Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<StructView<'a>> {
        match self.types.lookup(ty) {
            Type::Struct(struct_info) => Ok(struct_info),
            _ => {
                self.diag_ctx.emit_expected_struct_type_arg(builtin, ty, self.loc(span));
                Err(Poisoned)
            }
        }
    }

    fn materialize_as_local(&mut self, state: LocalState, ty: TypeId) -> mir::LocalId {
        match state {
            LocalState::Runtime(local) => local,
            LocalState::Comptime(vid) => {
                let target = self.mir_types.push(ty);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(vid) });
                target
            }
        }
    }
}

fn fold_runtime_builtin(
    builtin: RuntimeBuiltin,
    args: &[ValueId],
    values: &mut ValueInterner,
) -> ValueId {
    match *args {
        [a] => {
            let a = as_u256(values, a);
            match builtin {
                RuntimeBuiltin::IsZero => plank_evm::iszero(a).into(),
                RuntimeBuiltin::Not => values.intern_num(plank_evm::not(a)),
                _ => unreachable!("not a unary foldable builtin: {builtin}"),
            }
        }
        [a, b] => {
            let a = as_u256(values, a);
            let b = as_u256(values, b);
            match builtin {
                RuntimeBuiltin::Add => values.intern_num(plank_evm::add(a, b)),
                RuntimeBuiltin::Mul => values.intern_num(plank_evm::mul(a, b)),
                RuntimeBuiltin::Sub => values.intern_num(plank_evm::sub(a, b)),
                RuntimeBuiltin::Div => values.intern_num(plank_evm::div(a, b)),
                RuntimeBuiltin::SDiv => values.intern_num(plank_evm::sdiv(a, b)),
                RuntimeBuiltin::Mod => values.intern_num(plank_evm::r#mod(a, b)),
                RuntimeBuiltin::SMod => values.intern_num(plank_evm::smod(a, b)),
                RuntimeBuiltin::Exp => values.intern_num(plank_evm::exp(a, b)),
                RuntimeBuiltin::SignExtend => values.intern_num(plank_evm::signextend(a, b)),
                RuntimeBuiltin::Lt => plank_evm::lt(a, b).into(),
                RuntimeBuiltin::Gt => plank_evm::gt(a, b).into(),
                RuntimeBuiltin::SLt => plank_evm::slt(a, b).into(),
                RuntimeBuiltin::SGt => plank_evm::sgt(a, b).into(),
                RuntimeBuiltin::Eq => plank_evm::eq(a, b).into(),
                RuntimeBuiltin::And => values.intern_num(plank_evm::and(a, b)),
                RuntimeBuiltin::Or => values.intern_num(plank_evm::or(a, b)),
                RuntimeBuiltin::Xor => values.intern_num(plank_evm::xor(a, b)),
                RuntimeBuiltin::Byte => values.intern_num(plank_evm::byte(a, b)),
                RuntimeBuiltin::Shl => values.intern_num(plank_evm::shl(a, b)),
                RuntimeBuiltin::Shr => values.intern_num(plank_evm::shr(a, b)),
                RuntimeBuiltin::Sar => values.intern_num(plank_evm::sar(a, b)),
                _ => unreachable!("not a binary foldable builtin: {builtin}"),
            }
        }
        [a, b, c] => {
            let a = as_u256(values, a);
            let b = as_u256(values, b);
            let c = as_u256(values, c);
            match builtin {
                RuntimeBuiltin::AddMod => values.intern_num(plank_evm::addmod(a, b, c)),
                RuntimeBuiltin::MulMod => values.intern_num(plank_evm::mulmod(a, b, c)),
                _ => unreachable!("not a ternary foldable builtin: {builtin}"),
            }
        }
        _ => unreachable!("non-foldable builtin cannot be evaluated: {builtin}"),
    }
}

fn validate_uninit_type(
    ty: TypeId,
    types: &TypeInterner,
    diag_ctx: &mut DiagCtx<'_>,
    loc: SrcLoc,
    field_loc: Option<SrcLoc>,
) -> bool {
    match ty.as_primitive() {
        Ok(
            PrimitiveType::U256
            | PrimitiveType::Bool
            | PrimitiveType::MemoryPointer
            | PrimitiveType::Void
            | PrimitiveType::Type,
        ) => false,
        Ok(invalid @ (PrimitiveType::Function | PrimitiveType::Never)) => {
            // `field_loc` is set when recursing into struct fields
            if let Some(field_loc) = field_loc {
                diag_ctx.emit_invalid_uninit_struct_field(invalid, loc, field_loc);
            } else {
                diag_ctx.emit_invalid_uninit_type(invalid, loc);
            }
            true
        }
        Err(struct_ref) => {
            let view = types.lookup_struct(struct_ref);
            let mut has_invalid_uninit = false;
            for field in view.fields {
                let field_loc = SrcLoc::new(view.def_loc.source, field.def_span);
                has_invalid_uninit |=
                    validate_uninit_type(field.ty, types, diag_ctx, loc, Some(field_loc));
            }
            has_invalid_uninit
        }
    }
}

fn contains_memptr(ty: TypeId, types: &TypeInterner) -> bool {
    match ty.as_primitive() {
        Ok(PrimitiveType::MemoryPointer) => true,
        Ok(_) => false,
        Err(struct_ref) => {
            let view = types.lookup_struct(struct_ref);
            view.fields.iter().any(|f| contains_memptr(f.ty, types))
        }
    }
}

fn build_uninit_comptime(
    ty: TypeId,
    types: &TypeInterner,
    values: &mut ValueInterner,
    buf: &mut Vec<ValueId>,
) -> ValueId {
    match ty.as_primitive() {
        Ok(PrimitiveType::U256) => values.intern(Value::BigNum(U256::ZERO)),
        Ok(PrimitiveType::Bool) => values.intern(Value::Bool(false)),
        Ok(PrimitiveType::Void) => values.intern(Value::Void),
        Ok(PrimitiveType::Type) => values.intern(Value::Type(TypeId::VOID)),
        Ok(PrimitiveType::MemoryPointer | PrimitiveType::Function | PrimitiveType::Never) => {
            unreachable!("memptr/function/never cannot appear in comptime uninit struct")
        }
        Err(struct_ref) => {
            let buf_offset = buf.len();
            let view = types.lookup_struct(struct_ref);
            for field in view.fields {
                let vid = build_uninit_comptime(field.ty, types, values, buf);
                buf.push(vid);
            }
            let result = values.intern(Value::StructVal { ty, fields: &buf[buf_offset..] });
            buf.truncate(buf_offset);
            result
        }
    }
}

fn as_u256(values: &ValueInterner, vid: ValueId) -> U256 {
    match values.lookup(vid) {
        Value::BigNum(n) => n,
        other => unreachable!("invariant: type checked as u256, got {other:?}"),
    }
}
