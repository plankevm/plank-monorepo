use alloy_primitives::U256;
use plank_hir as hir;
use plank_mir as mir;
use plank_session::{EvmBuiltin, MaybePoisoned, SrcLoc};
use plank_values::{TypeId, Value, ValueId, ValueInterner};

use crate::scope::{BlockDiverge, EvalError, EvalResult, Ref, Scope};
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
        expr_loc: SrcLoc,
    ) -> EvalResult {
        self.with_types_buf(|this, types_buf_offset| {
            let args = &this.hir.call_args[args];
            for &arg in args {
                this.eval.types_buf.push(this.binding_type(arg)?);
            }
            let arg_types = &this.eval.types_buf[types_buf_offset..];

            let result_type = match builtin.resolve_result_type(arg_types) {
                Some(ty) => ty,
                None => {
                    this.eval.diag_ctx.emit_no_matching_builtin_signature(
                        &this.eval.types,
                        builtin,
                        &this.eval.types_buf[types_buf_offset..],
                        expr_loc,
                    );
                    return Err(EvalError::Poisoned);
                }
            };

            if builtin.is_pure() {
                let folded = this.with_values_buf::<MaybePoisoned<_>>(|this, values_buf_offset| {
                    for &arg in args {
                        match this.bindings[arg].state? {
                            Ref::Comptime(vid) => this.values_buf.push(vid),
                            Ref::Runtime(_) => return Ok(None),
                        }
                    }
                    Ok(Some(fold_pure_builtin(
                        builtin,
                        &this.eval.values_buf[values_buf_offset..],
                        this.eval.values,
                    )))
                });
                if let Some(folded) = folded? {
                    return Ok(Ref::Comptime(folded));
                }
            }
            if this.is_comptime() {
                this.diag_ctx.emit_unsupported_eval_of_evm_builtin(builtin, expr_loc);
                return Err(Poisoned.into());
            }

            let args = this.with_locals_buf(|this, locals_buf_offset| {
                for &arg in args {
                    let r#ref = this.bindings[arg]
                        .state
                        .expect("poisoned bindings filtered out during type collection above");
                    let arg = this.ensure_materialized(r#ref);
                    this.locals_buf.push(arg);
                }
                this.eval.mir_args.push_copy_slice(&this.eval.locals_buf[locals_buf_offset..])
            });
            let target = this.alloc_anon_mir(result_type);
            let expr = mir::Expr::BuiltinCall { builtin, args };
            this.instr_stack_buf.push(mir::Instruction::Set { target, expr });

            if result_type == TypeId::NEVER {
                return Err(EvalError::NEVER);
            }
            Ok(Ref::Runtime(target))
        })
    }
}
