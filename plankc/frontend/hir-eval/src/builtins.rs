use alloy_primitives::U256;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{Builtin, MaybePoisoned, RuntimeBuiltin, SourceSpan, builtins::BuiltinKind};
use plank_values::{Type, TypeId, Value, ValueId, ValueInterner, builtins as builtin_sigs};

use crate::scope::{Diverge, EvalValue, LocalState, Scope};
use plank_session::Poisoned;

impl Scope<'_, '_> {
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
                    self.eval_comptime_polymorphic_builtin(builtin, args, expr_span)
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
                let ty = self.expect_type_arg(hir_args[0], builtin, expr_span)?;
                let is_struct = matches!(self.eval.types.lookup(ty), Type::Struct(_));
                Ok(Ok(EvalValue::Comptime(is_struct.into())))
            }
            Builtin::FieldCount => {
                let ty = self.expect_type_arg(hir_args[0], builtin, expr_span)?;
                self.validate_struct_type(ty, builtin, expr_span)?;
                let count = U256::from(self.struct_info(ty).fields.len());
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(count))))
            }
            _ => unreachable!("not a comptime builtin: {builtin}"),
        }
    }

    fn eval_comptime_polymorphic_builtin(
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
            _ => unreachable!("not a comptime polymorphic builtin: {builtin}"),
        }
    }

    fn eval_field_type(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let ty = self.expect_type_arg(args[0], builtin, expr_span)?;
        let (ty, index) = self.resolve_struct_field_index(ty, args[1], builtin, expr_span)?;
        let field_ty = self.struct_info(ty).fields[index].ty;
        Ok(Ok(EvalValue::Comptime(self.eval.values.intern_type(field_ty))))
    }

    fn eval_get_field(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let instance_state = self.bindings[args[0]].state?;
        let ty = self.state_type(instance_state);
        let (ty, index) = self.resolve_struct_field_index(ty, args[1], builtin, expr_span)?;
        let field_type = self.struct_info(ty).fields[index].ty;
        let field_index = u32::try_from(index).expect("field index fits in u32");

        match instance_state {
            LocalState::Comptime(vid) => {
                let Value::StructVal { ty: _, fields } = self.values.lookup(vid) else {
                    unreachable!("invariant: type checked as struct")
                };
                Ok(Ok(EvalValue::Comptime(fields[index])))
            }
            LocalState::Runtime(local) => Ok(Ok(EvalValue::Runtime {
                expr: mir::Expr::FieldAccess { object: local, field_index },
                result_type: field_type,
            })),
        }
    }

    fn eval_set_field(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let instance_state = self.bindings[args[0]].state?;
        let ty = self.state_type(instance_state);
        let (ty, index) = self.resolve_struct_field_index(ty, args[1], builtin, expr_span)?;

        let new_value_state = self.bindings[args[2]].state?;
        let expected_field_type = self.struct_info(ty).fields[index].ty;
        let actual_ty = self.state_type(new_value_state);
        if actual_ty != expected_field_type {
            let field_def_loc = self.loc(self.struct_info(ty).fields[index].def_span);
            self.diag_ctx.emit_type_mismatch(
                expected_field_type,
                field_def_loc,
                actual_ty,
                self.loc(self.bindings[args[2]].use_span),
                false,
            );
            return Err(Poisoned);
        }

        // Both comptime: pure comptime fold.
        if let (LocalState::Comptime(instance_vid), LocalState::Comptime(new_value_vid)) =
            (instance_state, new_value_state)
        {
            return Ok(self.with_values_buf(|this, values_buf_offset| {
                let Value::StructVal { ty: _, fields: old_fields } =
                    this.eval.values.lookup(instance_vid)
                else {
                    unreachable!("invariant: type checked as struct")
                };
                this.eval.values_buf.extend_from_slice(old_fields);
                this.eval.values_buf[values_buf_offset + index] = new_value_vid;
                let new_fields = &this.eval.values_buf[values_buf_offset..];
                Ok(EvalValue::Comptime(
                    this.eval.values.intern(Value::StructVal { ty, fields: new_fields }),
                ))
            }));
        }

        // At least one side is runtime: emit MIR.
        if self.eval.types.is_comptime_only(ty) {
            let struct_def_loc = self.struct_info(ty).def_loc;
            self.diag_ctx.emit_set_field_on_comptime_only_struct(
                ty,
                self.loc(self.bindings[args[2]].use_span),
                struct_def_loc,
            );
            return Err(Poisoned);
        }

        let field_count = self.struct_info(ty).fields.len();
        let instance_local = self.materialize_as_local(instance_state, ty);
        let mir_fields = self.with_locals_buf(|this, locals_buf_offset| {
            for field_idx in 0..field_count {
                let local = if field_idx == index {
                    this.materialize_as_local(new_value_state, expected_field_type)
                } else {
                    let ftype = this.struct_info(ty).fields[field_idx].ty;
                    let target = this.mir_types.push(ftype);
                    this.emit(mir::Instruction::Set {
                        target,
                        expr: mir::Expr::FieldAccess {
                            object: instance_local,
                            field_index: u32::try_from(field_idx).expect("field index fits in u32"),
                        },
                    });
                    target
                };
                this.locals_buf.push(local);
            }
            this.eval.mir_args.push_copy_slice(&this.eval.locals_buf[locals_buf_offset..])
        });

        Ok(Ok(EvalValue::Runtime {
            expr: mir::Expr::StructLit { ty, fields: mir_fields },
            result_type: ty,
        }))
    }

    fn resolve_struct_field_index(
        &mut self,
        ty: TypeId,
        index_arg: hir::LocalId,
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<(TypeId, usize)> {
        self.validate_struct_type(ty, builtin, expr_span)?;
        let index = self.expect_comptime_field_index(index_arg, builtin, expr_span)?;
        let field_count = self.struct_info(ty).fields.len();
        if index >= field_count {
            self.diag_ctx.emit_field_index_out_of_bounds(
                builtin,
                index,
                field_count,
                self.loc(expr_span),
            );
            return Err(Poisoned);
        }
        Ok((ty, index))
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
    ) -> MaybePoisoned<usize> {
        let state = self.bindings[arg_local].state?;
        if let LocalState::Comptime(vid) = state
            && let Value::BigNum(n) = self.values.lookup(vid)
        {
            return match usize::try_from(n) {
                Ok(index) => Ok(index),
                Err(_) => {
                    self.diag_ctx.emit_field_index_overflow(builtin, n, self.loc(span));
                    Err(Poisoned)
                }
            };
        }
        self.diag_ctx.emit_expected_comptime_arg(builtin, "field index", self.loc(span));
        Err(Poisoned)
    }

    fn validate_struct_type(
        &mut self,
        ty: TypeId,
        builtin: Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<()> {
        if matches!(self.eval.types.lookup(ty), Type::Struct(_)) {
            Ok(())
        } else {
            self.diag_ctx.emit_expected_struct_type_arg(builtin, ty, self.loc(span));
            Err(Poisoned)
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

    fn struct_info(&self, ty: TypeId) -> plank_values::StructView<'_> {
        let Type::Struct(view) = self.eval.types.lookup(ty) else {
            unreachable!("invariant: already validated as struct")
        };
        view
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

fn as_u256(values: &ValueInterner, vid: ValueId) -> U256 {
    match values.lookup(vid) {
        Value::BigNum(n) => n,
        other => unreachable!("expected U256 value, got {other:?}"),
    }
}
