use crate::builtins;
use plank_hir::{
    self as hir,
    operators::{BinaryOp, UnaryOp},
};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, RuntimeBuiltin, SourceId, SourceSpan, SrcLoc};
use plank_values::{PrimitiveType, TypeId, Value, ValueId};

use crate::{diagnostics::DiagCtx, evaluator::Evaluator, scope::*};

pub(crate) struct OperatorTable {
    binary: Vec<(BinaryOp, TypeId, ValueId)>,
    negate: Option<ValueId>,
}

impl OperatorTable {
    pub fn new() -> Self {
        Self { binary: Vec::new(), negate: None }
    }

    pub fn lookup_binary(&self, op: BinaryOp, ty: TypeId) -> Option<ValueId> {
        self.binary.iter().find_map(|&(o, t, r#impl)| (o == op && t == ty).then_some(r#impl))
    }

    pub(crate) fn with_std_ops<'a>(
        hir: &hir::Hir,
        core_ops_source: SourceId,
        evaluator: &mut Evaluator<'a>,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> Self {
        let mut table = Self::new();

        const U256_OPS: &[(BinaryOp, &str)] = &[
            (BinaryOp::Add, "checked_add"),
            (BinaryOp::Subtract, "checked_sub"),
            (BinaryOp::Mul, "checked_mul"),
            (BinaryOp::Mod, "checked_mod"),
            (BinaryOp::DivRoundPos, "checked_div_up"),
            (BinaryOp::DivRoundAwayFromZero, "checked_div_up"),
            (BinaryOp::DivRoundNeg, "checked_div_down"),
            (BinaryOp::DivRoundToZero, "checked_div_down"),
            (BinaryOp::GreaterEquals, "greater_equals"),
            (BinaryOp::LessEquals, "less_equals"),
        ];

        for &(op, name) in U256_OPS {
            let Some(value_id) = resolve_std_fn(hir, core_ops_source, name, evaluator, diag_ctx)
            else {
                continue;
            };
            table.binary.push((op, TypeId::U256, value_id));
        }

        if let Some(value_id) =
            resolve_std_fn(hir, core_ops_source, "neg_u256", evaluator, diag_ctx)
        {
            table.negate.replace(value_id);
        }

        table
    }
}

fn resolve_std_fn<'a>(
    hir: &hir::Hir,
    core_ops_source: SourceId,
    name: &str,
    evaluator: &mut Evaluator<'a>,
    diag_ctx: &mut DiagCtx<'a>,
) -> Option<ValueId> {
    let name_id = diag_ctx.session.intern(name);
    let Some(const_id) = hir.consts.iter_idx().find(|id| {
        let def = hir.consts[*id];
        def.name == name_id && def.source_id == core_ops_source
    }) else {
        diag_ctx.emit_failed_to_resolve_std_fn(core_ops_source, name);
        return None;
    };

    let value_id = evaluator.evaluate_const(const_id, diag_ctx).ok()?;
    if !matches!(evaluator.values.lookup(value_id), Value::Closure { .. }) {
        diag_ctx.emit_std_operator_not_a_function(name, hir.consts[const_id].loc());
        return None;
    }
    Some(value_id)
}

impl crate::scope::Scope<'_, '_> {
    pub fn eval_binary_op(
        &mut self,
        op: BinaryOp,
        lhs: hir::LocalId,
        rhs: hir::LocalId,
        expr: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        if let Some(builtin) = op.runtime_equivalent() {
            let args = [lhs, rhs];
            return self.eval_runtime_foldable_builtin(builtin, &args, expr);
        }

        let lhs_binding = self.bindings[lhs];
        let rhs_binding = self.bindings[rhs];

        let lhs_state = lhs_binding.state?;
        let rhs_state = rhs_binding.state?;

        let lhs_ty = self.state_type(lhs_state);
        let rhs_ty = self.state_type(rhs_state);

        if lhs_ty != rhs_ty {
            self.diag_ctx.emit_operator_type_mismatch(lhs_ty, rhs_ty, self.loc(expr));
            return Err(Poisoned);
        }

        // `==` and `!=` are polymorphic over several types so we handle them separately.
        match op {
            BinaryOp::Equals => return self.eval_equality(true, lhs_ty, lhs, rhs, expr),
            BinaryOp::NotEquals => return self.eval_equality(false, lhs_ty, lhs, rhs, expr),
            _ => {}
        }

        let Some(closure_vid) = self.eval.operator_table.lookup_binary(op, lhs_ty) else {
            if lhs_ty == TypeId::MEMORY_POINTER && matches!(op, BinaryOp::Add | BinaryOp::Subtract)
            {
                self.diag_ctx.emit_operator_not_supported_for_memptr(op, self.loc(expr));
            } else {
                self.diag_ctx.emit_operator_not_supported(op, lhs_ty, self.loc(expr));
            }
            return Err(Poisoned);
        };

        let lhs_value = self.try_comptime(lhs_binding, expr)?;
        let rhs_value = self.try_comptime(rhs_binding, expr)?;
        if let Some((lhs, rhs)) = lhs_value.zip(rhs_value) {
            let value =
                fold_std_binary_op(op, lhs, rhs, self.eval.values, self.diag_ctx, self.loc(expr))?;
            return Ok(Ok(EvalValue::Comptime(value)));
        }

        self.with_captures_buf(|this, capture_buf_offset| {
            this.with_maybe_values_buf(|this, values_buf_offset| {
                let Value::Closure { fn_def, captures } = this.eval.values.lookup(closure_vid)
                else {
                    unreachable!("invariant: verified in build_table")
                };
                for &capture in captures {
                    this.eval.captures_buf.push(capture);
                }
                let arg_spans = this
                    .eval
                    .call_arg_spans
                    .push_copy_slice(&[lhs_binding.use_span, rhs_binding.use_span]);
                let args = [lhs, rhs];
                let res = this.eval_call_inner(
                    closure_vid,
                    fn_def,
                    &args,
                    arg_spans,
                    expr,
                    capture_buf_offset,
                    values_buf_offset,
                );
                this.eval.call_arg_spans.pop();
                res
            })
        })
    }

    pub fn eval_unary_op(
        &mut self,
        op: UnaryOp,
        input: hir::LocalId,
        expr: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        match op {
            UnaryOp::BitwiseNot => {
                return self.eval_runtime_foldable_builtin(
                    RuntimeBuiltin::Not,
                    std::array::from_ref(&input),
                    expr,
                );
            }
            UnaryOp::Negate => {}
        }

        let binding = self.bindings[input];
        let (state, use_span, _origin) = binding.poisoned()?;
        let ty = self.state_type(state);

        let r#impl = self.eval.operator_table.negate.filter(|_| ty.is_assignable_to(TypeId::U256));
        let Some(closure_vid) = r#impl else {
            self.diag_ctx.emit_operator_not_supported(op, ty, self.loc(expr));
            return Err(Poisoned);
        };

        if let Some(input_vid) = self.try_comptime(binding, expr)? {
            let a = builtins::as_u256(self.eval.values, input_vid);
            let res = alloy_primitives::U256::ZERO.wrapping_sub(a);
            return Ok(Ok(EvalValue::Comptime(self.eval.values.intern_num(res))));
        }

        self.with_captures_buf(|this, capture_buf_offset| {
            this.with_maybe_values_buf(|this, values_buf_offset| {
                let Value::Closure { fn_def: fn_def_id, captures } =
                    this.eval.values.lookup(closure_vid)
                else {
                    unreachable!("invariant: verified in build_table")
                };
                for &capture in captures {
                    this.eval.captures_buf.push(capture);
                }
                let arg_spans = this.eval.call_arg_spans.push_copy_slice(&[use_span]);
                let args = [input];
                let res = this.eval_call_inner(
                    closure_vid,
                    fn_def_id,
                    &args,
                    arg_spans,
                    expr,
                    capture_buf_offset,
                    values_buf_offset,
                );
                this.eval.call_arg_spans.pop();
                res
            })
        })
    }

    fn eval_equality(
        &mut self,
        op_equals: bool,
        ty: TypeId,
        lhs: hir::LocalId,
        rhs: hir::LocalId,
        expr: SourceSpan,
    ) -> MaybePoisoned<Result<EvalValue, Diverge>> {
        let lhs_binding = self.bindings[lhs];
        let rhs_binding = self.bindings[rhs];
        let (lhs_state, _, _) = lhs_binding.poisoned()?;
        let (rhs_state, _, _) = rhs_binding.poisoned()?;

        match (op_equals, ty.as_primitive()) {
            (op_equals, Ok(PrimitiveType::Type)) => {
                let (LocalState::Comptime(lhs), LocalState::Comptime(rhs)) = (lhs_state, rhs_state)
                else {
                    unreachable!("invariant: type is comptime-only")
                };
                let result = if op_equals { lhs == rhs } else { lhs != rhs };
                Ok(Ok(EvalValue::Comptime(result.into())))
            }
            (
                true,
                Ok(PrimitiveType::U256 | PrimitiveType::MemoryPointer | PrimitiveType::Bool),
            ) => {
                let args = [lhs, rhs];
                self.eval_runtime_foldable_builtin(RuntimeBuiltin::Eq, &args, expr)
            }
            (op_equals, Ok(PrimitiveType::Void)) => {
                // `(x: void) == (y: void)` is always `true`, `!=` always `false`.
                Ok(Ok(EvalValue::Comptime(op_equals.into())))
            }
            (false, Ok(PrimitiveType::U256 | PrimitiveType::MemoryPointer)) => {
                let lhs_value = self.try_comptime(lhs_binding, expr)?;
                let rhs_value = self.try_comptime(rhs_binding, expr)?;
                if let Some((lhs, rhs)) = lhs_value.zip(rhs_value) {
                    return Ok(Ok(EvalValue::Comptime((lhs != rhs).into())));
                }
                // Lower `a != b` as `iszero(eq(a, b))`.
                let lhs = self.materialize_as_local(lhs_state, ty);
                let rhs = self.materialize_as_local(rhs_state, ty);
                let args = self.eval.mir_args.push_copy_slice(&[lhs, rhs]);
                let is_eq_target = self.mir_types.push(TypeId::BOOL);
                self.emit(mir::Instruction::Set {
                    target: is_eq_target,
                    expr: mir::Expr::RuntimeBuiltinCall { builtin: RuntimeBuiltin::Eq, args },
                });
                Ok(Ok(EvalValue::Runtime {
                    expr: mir::Expr::RuntimeBuiltinCall {
                        builtin: RuntimeBuiltin::IsZero,
                        args: self.mir_args.push_copy_slice(&[is_eq_target]),
                    },
                    result_type: TypeId::BOOL,
                }))
            }
            (false, Ok(PrimitiveType::Bool)) => {
                let lhs_value = self.try_comptime(lhs_binding, expr)?;
                let rhs_value = self.try_comptime(rhs_binding, expr)?;
                if let Some((lhs, rhs)) = lhs_value.zip(rhs_value) {
                    return Ok(Ok(EvalValue::Comptime((lhs != rhs).into())));
                }
                let lhs = self.materialize_as_local(lhs_state, ty);
                let rhs = self.materialize_as_local(rhs_state, ty);
                let args = self.eval.mir_args.push_copy_slice(&[lhs, rhs]);
                Ok(Ok(EvalValue::Runtime {
                    expr: mir::Expr::RuntimeBuiltinCall { builtin: RuntimeBuiltin::Xor, args },
                    result_type: TypeId::BOOL,
                }))
            }
            (op_equals, Err(_) | Ok(PrimitiveType::Function | PrimitiveType::Never)) => {
                let op = if op_equals { BinaryOp::Equals } else { BinaryOp::NotEquals };
                self.diag_ctx.emit_operator_not_supported(op, ty, self.loc(expr));
                Err(Poisoned)
            }
        }
    }

    pub fn eval_logical_not(&mut self, local: hir::LocalId) -> MaybePoisoned<EvalValue> {
        let (state, use_span, _origin) = self.bindings[local].poisoned()?;
        let ty = self.state_type(state);
        if !ty.is_assignable_to(TypeId::BOOL) {
            self.diag_ctx.emit_type_mismatch_simple(TypeId::BOOL, ty, self.loc(use_span));
            return Err(Poisoned);
        }
        let value = match state {
            LocalState::Runtime(mir) => {
                let args = self.mir_args.push_copy_slice(&[mir]);
                EvalValue::Runtime {
                    expr: mir::Expr::RuntimeBuiltinCall { builtin: RuntimeBuiltin::IsZero, args },
                    result_type: TypeId::BOOL,
                }
            }
            LocalState::Comptime(ValueId::FALSE) => EvalValue::Comptime(ValueId::TRUE),
            LocalState::Comptime(ValueId::TRUE) => EvalValue::Comptime(ValueId::FALSE),
            LocalState::Comptime(_) => unreachable!("already type checked"),
        };
        Ok(value)
    }
}

fn fold_std_binary_op(
    op: BinaryOp,
    lhs: ValueId,
    rhs: ValueId,
    values: &mut plank_values::ValueInterner,
    diag_ctx: &mut DiagCtx,
    loc: SrcLoc,
) -> MaybePoisoned<ValueId> {
    use alloy_primitives::U256;

    let a = builtins::as_u256(values, lhs);
    let b = builtins::as_u256(values, rhs);

    let value = match op {
        BinaryOp::Add => {
            let (res, overflow) = a.overflowing_add(b);
            if overflow {
                diag_ctx.emit_comptime_arithmetic_overflow(op, loc);
                return Err(Poisoned);
            }
            values.intern_num(res)
        }
        BinaryOp::Subtract => {
            let (res, overflow) = a.overflowing_sub(b);
            if overflow {
                diag_ctx.emit_comptime_arithmetic_underflow(op, loc);
                return Err(Poisoned);
            }
            values.intern_num(res)
        }
        BinaryOp::Mul => {
            let (res, overflow) = a.overflowing_mul(b);
            if overflow {
                diag_ctx.emit_comptime_arithmetic_overflow(op, loc);
                return Err(Poisoned);
            }
            values.intern_num(res)
        }
        BinaryOp::Mod => {
            let Some(rem) = a.checked_rem(b) else {
                diag_ctx.emit_comptime_modulo_by_zero(op, loc);
                return Err(Poisoned);
            };
            values.intern_num(rem)
        }
        BinaryOp::DivRoundPos | BinaryOp::DivRoundAwayFromZero => {
            let Some(rem) = a.checked_rem(b) else {
                diag_ctx.emit_comptime_division_by_zero(op, loc);
                return Err(Poisoned);
            };
            let mut res = a / b;
            if !rem.is_zero() {
                res += U256::ONE;
            }
            values.intern_num(res)
        }
        BinaryOp::DivRoundNeg | BinaryOp::DivRoundToZero => {
            let Some(res) = a.checked_div(b) else {
                diag_ctx.emit_comptime_division_by_zero(op, loc);
                return Err(Poisoned);
            };
            values.intern_num(res)
        }
        BinaryOp::GreaterEquals => (a >= b).into(),
        BinaryOp::LessEquals => (a <= b).into(),

        BinaryOp::NotEquals
        | BinaryOp::Equals
        | BinaryOp::LessThan
        | BinaryOp::GreaterThan
        | BinaryOp::BitwiseOr
        | BinaryOp::BitwiseXor
        | BinaryOp::BitwiseAnd
        | BinaryOp::ShiftLeft
        | BinaryOp::ShiftRight
        | BinaryOp::AddWrap
        | BinaryOp::SubtractWrap
        | BinaryOp::MulWrap => unreachable!("not a std binary op: {op:?}"),
    };
    Ok(value)
}
