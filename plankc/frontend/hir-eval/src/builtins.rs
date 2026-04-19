use alloy_primitives::U256;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{EvmBuiltin, MaybePoisoned, SourceSpan};
use plank_values::{TypeId, Value, ValueId, ValueInterner, builtins::resolve_result_type};

use crate::scope::{Diverge, EvalValue, LocalState, Scope};
use plank_session::Poisoned;

fn as_u256(values: &ValueInterner, vid: ValueId) -> U256 {
    match values.lookup(vid) {
        Value::BigNum(n) => n,
        other => unreachable!("expected U256 value, got {other:?}"),
    }
}

pub(crate) fn fold_pure_builtin(
    builtin: EvmBuiltin,
    args: &[ValueId],
    values: &mut ValueInterner,
) -> ValueId {
    use EvmBuiltin::*;

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
        builtin: EvmBuiltin,
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

            resolve_result_type(builtin, arg_types).ok_or_else(|| {
                this.diag_ctx.emit_no_matching_builtin_signature(builtin, arg_types, expr_loc);
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
                self.diag_ctx.emit_unsupported_eval_of_evm_builtin(builtin, expr_loc);
                if result_type == TypeId::NEVER {
                    return Ok(Err(Diverge::END));
                } else {
                    return Err(Poisoned);
                }
            }
        }

        let args = self.eval.mir_args.push_with(|mut mir_args| {
            for &arg in args {
                let state =
                    self.bindings[arg].state.expect("invariant: arg type check checks poison");
                let arg = match state {
                    LocalState::Comptime(vid) => {
                        let ty = self.eval.values.type_of_value(vid);
                        assert!(
                            !self.eval.types.is_comptime_only(ty),
                            "evm builtin typechecks for comptime only value"
                        );
                        let target = self.mir_types.push(self.eval.values.type_of_value(vid));
                        self.eval
                            .instr_stack_buf
                            .push(mir::Instruction::Set { target, expr: mir::Expr::Const(vid) });
                        target
                    }
                    LocalState::Runtime(local) => local,
                };
                mir_args.push(arg);
            }
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
}
