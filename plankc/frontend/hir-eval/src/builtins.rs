use alloy_primitives::U256;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{
    Builtin, ComptimeBuiltin, MaybePoisoned, PolymorphicBuiltin, RuntimeBuiltin, SourceSpan,
};
use plank_values::{Type, TypeId, Value, ValueId, ValueInterner};

use crate::scope::{Diverge, EvalValue, LocalState, Scope};
use plank_session::Poisoned;

fn as_u256(values: &ValueInterner, vid: ValueId) -> U256 {
    match values.lookup(vid) {
        Value::BigNum(n) => n,
        other => unreachable!("expected U256 value, got {other:?}"),
    }
}

pub(crate) fn fold_pure_builtin(
    builtin: RuntimeBuiltin,
    args: &[ValueId],
    values: &mut ValueInterner,
) -> ValueId {
    use RuntimeBuiltin::*;

    match *args {
        [a] => {
            let a = as_u256(values, a);
            match builtin {
                IsZero => plank_evm::iszero(a).into(),
                Not => values.intern_num(plank_evm::not(a)),
                _ => unreachable!("not a unary pure builtin: {builtin}"),
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
                _ => unreachable!("not a binary pure builtin: {builtin}"),
            }
        }
        [a, b, c] => {
            let a = as_u256(values, a);
            let b = as_u256(values, b);
            let c = as_u256(values, c);
            match builtin {
                AddMod => values.intern_num(plank_evm::addmod(a, b, c)),
                MulMod => values.intern_num(plank_evm::mulmod(a, b, c)),
                _ => unreachable!("not a ternary pure builtin: {builtin}"),
            }
        }
        _ => unreachable!("impure builtin cannot be evaluated: {builtin}"),
    }
}

impl Scope<'_, '_> {
    pub(crate) fn eval_builtin(
        &mut self,
        builtin: RuntimeBuiltin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let args = &self.hir.call_args[args];
        let expr_loc = self.loc(expr_span);

        let result_type = self.with_types_buf(|this, types_buf_offset| {
            for &arg in args {
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
        })?;

        if builtin.is_pure() {
            let folded = self.with_values_buf(|this, values_buf_offset| {
                for &arg in args {
                    let (state, arg_def_span) = this.bindings[arg]
                        .poisoned()
                        .expect("invariant: arg type check checks poison");
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
                Ok(Some(fold_pure_builtin(
                    builtin,
                    &this.eval.values_buf[values_buf_offset..],
                    this.eval.values,
                )))
            })?;
            if let Some(value) = folded {
                return Ok(Ok(EvalValue::Comptime(value)));
            }
        } else {
            if self.is_comptime() {
                self.diag_ctx.emit_unsupported_eval_of_runtime_builtin(builtin, expr_loc);
                if result_type == TypeId::NEVER {
                    return Ok(Err(Diverge::PoisonedControlFlow));
                } else {
                    return Err(Poisoned);
                }
            }
        }

        let args = self.with_locals_buf(|this, locals_buf_offset| {
            for &arg in args {
                let state =
                    this.bindings[arg].state.expect("invariant: arg type check checks poison");
                let arg = match state {
                    LocalState::Comptime(vid) => {
                        assert!(
                            !this.is_comptime_only(vid),
                            "evm builtin typechecks for comptime only value"
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

        let expr = mir::Expr::BuiltinCall { builtin, args };
        if result_type == TypeId::NEVER {
            // We diverge after this so we need to make sure the call is actually included.
            let target = self.mir_types.push(result_type);
            self.emit(mir::Instruction::Set { target, expr });
            return Ok(Err(Diverge::BlockEnd(None)));
        }

        Ok(Ok(EvalValue::Runtime { expr, result_type }))
    }

    pub(crate) fn eval_comptime_builtin(
        &mut self,
        builtin: ComptimeBuiltin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let args = &self.hir.call_args[args];
        let expr_loc = self.loc(expr_span);

        if builtin.arg_count() != args.len() {
            self.with_types_buf(|this, types_buf_offset| {
                for &arg in args {
                    let ty = this.state_type(this.bindings[arg].state?);
                    this.eval.types_buf.push(ty);
                }
                let arg_types = &this.eval.types_buf[types_buf_offset..];
                this.diag_ctx.emit_no_matching_builtin_signature(
                    &this.eval.types,
                    builtin,
                    arg_types,
                    expr_loc,
                );
                Err(Poisoned)
            })?;
        }

        match builtin {
            ComptimeBuiltin::IsStruct => {
                let ty = self.expect_type_arg(args[0], builtin, expr_span)?;
                let is_struct = matches!(self.eval.types.lookup(ty), Type::Struct(_));
                Ok(Ok(EvalValue::Comptime(is_struct.into())))
            }
            ComptimeBuiltin::FieldCount => {
                let ty = self.expect_type_arg(args[0], builtin, expr_span)?;
                self.validate_struct_type(ty, builtin, expr_span)?;
                let count = U256::from(self.struct_field_count(ty));
                Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(count))))
            }
        }
    }

    /// Extracts a `TypeId` from an evaluated arg local.
    /// Emits a diagnostic and returns `Err(Poisoned)` if the arg is not a type value.
    fn expect_type_arg(
        &mut self,
        arg_local: hir::LocalId,
        builtin: impl Builtin,
        span: SourceSpan,
    ) -> MaybePoisoned<TypeId> {
        let state = self.bindings[arg_local].state?;
        match state {
            LocalState::Comptime(vid) => match self.values.lookup(vid) {
                Value::Type(ty) => Ok(ty),
                _ => {
                    let actual_ty = self.state_type(state);
                    self.diag_ctx.emit_expected_type_arg(
                        &self.eval.types,
                        builtin,
                        actual_ty,
                        self.loc(span),
                    );
                    Err(Poisoned)
                }
            },
            LocalState::Runtime(_) => {
                let actual_ty = self.state_type(state);
                self.diag_ctx.emit_expected_type_arg(
                    &self.eval.types,
                    builtin,
                    actual_ty,
                    self.loc(span),
                );
                Err(Poisoned)
            }
        }
    }

    pub(crate) fn eval_polymorphic_builtin(
        &mut self,
        builtin: PolymorphicBuiltin,
        args: hir::CallArgsId,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let args = &self.hir.call_args[args];
        let expr_loc = self.loc(expr_span);

        if builtin.arg_count() != args.len() {
            self.diag_ctx.emit_wrong_arg_count(&self.eval.types, builtin, args.len(), expr_loc);
            return Err(Poisoned);
        }

        match builtin {
            PolymorphicBuiltin::FieldType => self.eval_field_type(args, builtin, expr_span),
            PolymorphicBuiltin::GetField => self.eval_get_field(args, builtin, expr_span),
            PolymorphicBuiltin::SetField => self.eval_set_field(args, builtin, expr_span),
        }
    }

    /// Validates a struct type argument and field index, returning both.
    fn validate_struct_field_index(
        &mut self,
        args: &[hir::LocalId],
        builtin: PolymorphicBuiltin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<(TypeId, usize)> {
        let ty = self.expect_type_arg(args[0], builtin, expr_span)?;
        let index = self.expect_comptime_field_index(args[1], builtin, expr_span)?;
        self.validate_struct_type(ty, builtin, expr_span)?;
        let field_count = self.struct_field_count(ty);
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

    fn eval_field_type(
        &mut self,
        args: &[hir::LocalId],
        builtin: PolymorphicBuiltin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let (ty, index) = self.validate_struct_field_index(args, builtin, expr_span)?;
        let (_name, field_ty) = self.struct_field(ty, index);
        Ok(Ok(EvalValue::Comptime(self.eval.values.intern_type(field_ty))))
    }

    fn eval_get_field(
        &mut self,
        args: &[hir::LocalId],
        builtin: PolymorphicBuiltin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let (ty, index) = self.validate_struct_field_index(args, builtin, expr_span)?;
        let (_name, field_type) = self.struct_field(ty, index);
        let field_index = index as u32;
        let instance_state = self.bindings[args[2]].state?;

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
        builtin: PolymorphicBuiltin,
        expr_span: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let (ty, index) = self.validate_struct_field_index(args, builtin, expr_span)?;
        let field_count = self.struct_field_count(ty);
        let (_name, expected_field_type) = self.struct_field(ty, index);

        let new_value_state = self.bindings[args[3]].state?;
        let new_value_type = self.state_type(new_value_state);
        if new_value_type != expected_field_type {
            self.diag_ctx.emit_type_mismatch_simple(
                &self.eval.types,
                expected_field_type,
                new_value_type,
                self.loc(expr_span),
            );
            return Err(Poisoned);
        }

        let instance_state = self.bindings[args[2]].state?;

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

        let locals_buf_offset = self.eval.locals_buf.len();
        for i in 0..field_count {
            if i == index {
                let local = match new_value_state {
                    LocalState::Comptime(vid) => {
                        let target = self.mir_types.push(expected_field_type);
                        self.emit(mir::Instruction::Set { target, expr: mir::Expr::Const(vid) });
                        target
                    }
                    LocalState::Runtime(local) => local,
                };
                self.eval.locals_buf.push(local);
            } else {
                let (_fname, ftype) = self.struct_field(ty, i);
                let target = self.mir_types.push(ftype);
                self.emit(mir::Instruction::Set {
                    target,
                    expr: mir::Expr::FieldAccess { object: instance_local, field_index: i as u32 },
                });
                self.eval.locals_buf.push(target);
            }
        }
        let field_locals = &self.eval.locals_buf[locals_buf_offset..];
        let mir_fields = self.eval.mir_args.push_copy_slice(field_locals);
        self.eval.locals_buf.truncate(locals_buf_offset);

        Ok(Ok(EvalValue::Runtime {
            expr: mir::Expr::StructLit { ty, fields: mir_fields },
            result_type: ty,
        }))
    }

    /// Extracts a comptime field index from an arg.
    fn expect_comptime_field_index(
        &mut self,
        arg_local: hir::LocalId,
        builtin: PolymorphicBuiltin,
        span: SourceSpan,
    ) -> MaybePoisoned<usize> {
        let state = self.bindings[arg_local].state?;
        match state {
            LocalState::Comptime(vid) => match self.values.lookup(vid) {
                // On overflow, return usize::MAX — the bounds check in
                // validate_struct_field_index will catch it.
                Value::BigNum(n) => Ok(usize::try_from(n).unwrap_or(usize::MAX)),
                _ => {
                    self.diag_ctx.emit_expected_comptime_arg(
                        builtin,
                        "field index",
                        self.loc(span),
                    );
                    Err(Poisoned)
                }
            },
            LocalState::Runtime(_) => {
                self.diag_ctx.emit_expected_comptime_arg(builtin, "field index", self.loc(span));
                Err(Poisoned)
            }
        }
    }

    /// Validates that a `TypeId` refers to a struct type.
    fn validate_struct_type(
        &mut self,
        ty: TypeId,
        builtin: impl Builtin,
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

    fn struct_field_count(&self, ty: TypeId) -> usize {
        let Type::Struct(info) = self.eval.types.lookup(ty) else {
            unreachable!("invariant: already validated as struct")
        };
        info.fields.len()
    }

    fn struct_field(&self, ty: TypeId, index: usize) -> (plank_session::StrId, TypeId) {
        let Type::Struct(info) = self.eval.types.lookup(ty) else {
            unreachable!("invariant: already validated as struct")
        };
        info.fields[index]
    }
}
