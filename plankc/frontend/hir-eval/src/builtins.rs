use alloy_primitives::U256;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{Builtin, MaybePoisoned, RuntimeBuiltin, SourceSpan, builtins::BuiltinKind};
use plank_values::{Type, TypeId, Value, ValueId, ValueInterner};

use crate::scope::{Diverge, EvalValue, LocalState, Scope};
use plank_session::Poisoned;

impl Scope<'_, '_> {
    pub(crate) fn eval_builtin_call(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        match builtin.kind() {
            BuiltinKind::RuntimeFoldable(_) => {
                self.eval_runtime_foldable_builtin(builtin, args, expr_span)
            }
            BuiltinKind::RuntimeOnly(_) => self.eval_runtime_only_builtin(builtin, args, expr_span),
            BuiltinKind::Comptime(_) => self.eval_comptime_builtin(builtin, args, expr_span),
            BuiltinKind::ComptimePolymorphic { .. } => {
                self.eval_comptime_polymorphic_builtin(builtin, args, expr_span)
            }
        }
    }

    fn eval_runtime_foldable_builtin(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let result_type = self.resolve_runtime_builtin_result_type(builtin, args, expr_span)?;

        let hir_args = &self.hir.call_args[args];
        let folded = self.with_values_buf(|this, values_buf_offset| {
            for &arg in hir_args {
                let (state, arg_def_span) =
                    this.bindings[arg].poisoned().expect("invariant: arg type check checks poison");
                match state {
                    LocalState::Comptime(vid) => this.values_buf.push(vid),
                    LocalState::Runtime(_) if this.is_comptime() => {
                        this.diag_ctx.emit_runtime_ref_in_comptime(
                            this.source,
                            expr_span,
                            arg_def_span,
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
        builtin: Builtin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let result_type = self.resolve_runtime_builtin_result_type(builtin, args, expr_span)?;

        if self.is_comptime() {
            self.diag_ctx.emit_unsupported_eval_of_runtime_builtin(builtin, self.loc(expr_span));
            if result_type == TypeId::NEVER {
                return Ok(Err(Diverge::PoisonedControlFlow));
            } else {
                return Err(Poisoned);
            }
        }

        Ok(self.emit_runtime_builtin_mir(builtin, args, result_type))
    }

    fn resolve_runtime_builtin_result_type(
        &mut self,
        builtin: Builtin,
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

            builtin.resolve_result_type(arg_types).ok_or_else(|| {
                this.diag_ctx.emit_no_matching_builtin_signature(
                    &this.eval.types,
                    builtin,
                    &this.eval.types_buf[types_buf_offset..],
                    expr_loc,
                );
                Poisoned
            })
        })
    }

    fn emit_runtime_builtin_mir(
        &mut self,
        builtin: Builtin,
        args: hir::CallArgsId,
        result_type: TypeId,
    ) -> Result<EvalValue, Diverge> {
        let hir_args = &self.hir.call_args[args];
        let mir_args = self.with_locals_buf(|this, locals_buf_offset| {
            for &arg in hir_args {
                let state =
                    this.bindings[arg].state.expect("invariant: arg type check checks poison");
                let arg = match state {
                    LocalState::Comptime(vid) => {
                        assert!(
                            !this.is_comptime_only(vid),
                            "runtime builtin typechecks for comptime only value"
                        );
                        let target = this.mir_types.push(this.values.type_of_value(vid));
                        this.emit(plank_mir::Instruction::Set {
                            target,
                            expr: mir::Expr::Const(vid),
                        });
                        target
                    }
                    LocalState::Runtime(local) => local,
                };
                this.locals_buf.push(arg);
            }
            this.eval.mir_args.push_copy_slice(&this.eval.locals_buf[locals_buf_offset..])
        });

        let expr = mir::Expr::RuntimeBuiltinCall {
            builtin: RuntimeBuiltin::try_from(builtin)
                .expect("dispatched via RuntimeFoldable/RuntimeOnly kind"),
            args: mir_args,
        };
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

        if builtin.arg_count() != hir_args.len() {
            self.diag_ctx.emit_wrong_arg_count(&self.eval.types, builtin, hir_args.len(), expr_loc);
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

        if builtin.arg_count() != hir_args.len() {
            self.diag_ctx.emit_wrong_arg_count(&self.eval.types, builtin, hir_args.len(), expr_loc);
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
        let (ty, index) = self.resolve_struct_field_index(args, builtin, expr_span)?;
        let (_, field_ty) = self.struct_info(ty).fields[index];
        Ok(Ok(EvalValue::Comptime(self.eval.values.intern_type(field_ty))))
    }

    fn eval_get_field(
        &mut self,
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let (ty, index) = self.resolve_struct_field_index(args, builtin, expr_span)?;
        let (_, field_type) = self.struct_info(ty).fields[index];
        let field_index = u32::try_from(index).expect("field index fits in u32");
        let instance_state = self.bindings[args[2]].state?;
        let instance_type = self.state_type(instance_state);
        self.check_type_match(ty, instance_type, expr_span)?;

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
        let (ty, index) = self.resolve_struct_field_index(args, builtin, expr_span)?;
        let field_count = self.struct_info(ty).fields.len();
        let (_, expected_field_type) = self.struct_info(ty).fields[index];

        let instance_state = self.bindings[args[2]].state?;
        let instance_type = self.state_type(instance_state);
        self.check_type_match(ty, instance_type, expr_span)?;

        let new_value_state = self.bindings[args[3]].state?;
        let new_value_type = self.state_type(new_value_state);
        self.check_type_match(expected_field_type, new_value_type, expr_span)?;

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
        let instance_local = match instance_state {
            LocalState::Runtime(local) => local,
            LocalState::Comptime(vid) => {
                let target = self.mir_types.push(ty);
                self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(vid) });
                target
            }
        };

        let mir_fields = self.with_locals_buf(|this, locals_buf_offset| {
            for i in 0..field_count {
                if i == index {
                    let local = match new_value_state {
                        LocalState::Comptime(vid) => {
                            let target = this.mir_types.push(expected_field_type);
                            this.emit(mir::Instruction::Set {
                                target,
                                expr: mir::Expr::Const(vid),
                            });
                            target
                        }
                        LocalState::Runtime(local) => local,
                    };
                    this.locals_buf.push(local);
                } else {
                    let (_, ftype) = this.struct_info(ty).fields[i];
                    let target = this.mir_types.push(ftype);
                    this.emit(mir::Instruction::Set {
                        target,
                        expr: mir::Expr::FieldAccess {
                            object: instance_local,
                            field_index: u32::try_from(i).expect("field index fits in u32"),
                        },
                    });
                    this.locals_buf.push(target);
                }
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
        args: &[hir::LocalId],
        builtin: Builtin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<(TypeId, usize)> {
        let ty = self.expect_type_arg(args[0], builtin, expr_span)?;
        let index = self.expect_comptime_field_index(args[1], builtin, expr_span)?;
        self.validate_struct_type(ty, builtin, expr_span)?;
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
        self.diag_ctx.emit_expected_type_arg(&self.eval.types, builtin, actual_ty, self.loc(span));
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
            // On overflow, return usize::MAX — the bounds check in
            // resolve_struct_field_index will catch it.
            return Ok(usize::try_from(n).unwrap_or(usize::MAX));
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
            self.diag_ctx.emit_expected_struct_type_arg(
                &self.eval.types,
                builtin,
                ty,
                self.loc(span),
            );
            Err(Poisoned)
        }
    }

    fn check_type_match(
        &mut self,
        expected: TypeId,
        actual: TypeId,
        span: SourceSpan,
    ) -> MaybePoisoned<()> {
        if expected != actual {
            self.diag_ctx.emit_type_mismatch_simple(
                &self.eval.types,
                expected,
                actual,
                self.loc(span),
            );
            return Err(Poisoned);
        }
        Ok(())
    }

    fn struct_info(&self, ty: TypeId) -> plank_values::StructInfo<'_> {
        let Type::Struct(info) = self.eval.types.lookup(ty) else {
            unreachable!("invariant: already validated as struct")
        };
        info
    }
}

fn fold_runtime_builtin(builtin: Builtin, args: &[ValueId], values: &mut ValueInterner) -> ValueId {
    use Builtin::*;

    match *args {
        [a] => {
            let a = as_u256(values, a);
            match builtin {
                IsZero => plank_evm::iszero(a).into(),
                Not => values.intern_num(plank_evm::not(a)),
                _ => unreachable!("not a unary foldable builtin: {builtin}"),
            }
        }
        [a, b] => {
            let a = as_u256(values, a);
            let b = as_u256(values, b);
            match builtin {
                Add => values.intern_num(plank_evm::add(a, b)),
                Mul => values.intern_num(plank_evm::mul(a, b)),
                Sub => values.intern_num(plank_evm::sub(a, b)),
                Div => values.intern_num(plank_evm::div(a, b)),
                SDiv => values.intern_num(plank_evm::sdiv(a, b)),
                Mod => values.intern_num(plank_evm::r#mod(a, b)),
                SMod => values.intern_num(plank_evm::smod(a, b)),
                Exp => values.intern_num(plank_evm::exp(a, b)),
                SignExtend => values.intern_num(plank_evm::signextend(a, b)),
                Lt => plank_evm::lt(a, b).into(),
                Gt => plank_evm::gt(a, b).into(),
                SLt => plank_evm::slt(a, b).into(),
                SGt => plank_evm::sgt(a, b).into(),
                Eq => plank_evm::eq(a, b).into(),
                And => values.intern_num(plank_evm::and(a, b)),
                Or => values.intern_num(plank_evm::or(a, b)),
                Xor => values.intern_num(plank_evm::xor(a, b)),
                Byte => values.intern_num(plank_evm::byte(a, b)),
                Shl => values.intern_num(plank_evm::shl(a, b)),
                Shr => values.intern_num(plank_evm::shr(a, b)),
                Sar => values.intern_num(plank_evm::sar(a, b)),
                _ => unreachable!("not a binary foldable builtin: {builtin}"),
            }
        }
        [a, b, c] => {
            let a = as_u256(values, a);
            let b = as_u256(values, b);
            let c = as_u256(values, c);
            match builtin {
                AddMod => values.intern_num(plank_evm::addmod(a, b, c)),
                MulMod => values.intern_num(plank_evm::mulmod(a, b, c)),
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
