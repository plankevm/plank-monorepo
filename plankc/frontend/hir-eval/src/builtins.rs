use alloy_primitives::{U256, uint};
use plank_hir::{self as hir, CallArgsId};
use plank_session::{EvmBuiltin, SrcLoc};
use plank_values::ValueId;

use crate::{ComptimeInterpreter, Evaluator, value::Value};

trait InternableValue {
    fn intern(self, eval: &mut Evaluator<'_>) -> ValueId;
}

impl InternableValue for bool {
    fn intern(self, _eval: &mut Evaluator<'_>) -> ValueId {
        if self { ValueId::TRUE } else { ValueId::FALSE }
    }
}

impl InternableValue for U256 {
    fn intern(self, eval: &mut Evaluator<'_>) -> ValueId {
        let id = eval.big_nums.intern(self);
        eval.values.intern_num(id)
    }
}

impl ComptimeInterpreter {
    pub(crate) fn eval_evm_builtin(
        &mut self,
        eval: &mut Evaluator<'_>,
        builtin: EvmBuiltin,
        args: CallArgsId,
        loc: SrcLoc,
    ) -> ValueId {
        let arg_locals = &eval.hir.call_args[args];

        let arg_types_valid = self.type_buf.use_as(|arg_types| {
            for &local in arg_locals {
                arg_types.push(eval.values.type_of_value(self.bindings[local].0));
            }
            'sig: for &(input_types, _result_type) in builtin.signatures() {
                if input_types.len() != arg_types.len() {
                    continue 'sig;
                }
                for (&expected, &actual) in input_types.iter().zip(arg_types.iter()) {
                    if !actual.is_assignable_to(expected) {
                        continue 'sig;
                    }
                }
                return true;
            }
            eval.emit_no_matching_builtin_signature(builtin, arg_types, loc);
            false
        });

        match builtin {
            // Non-pure: cannot be evaluated at comptime
            EvmBuiltin::Keccak256
            | EvmBuiltin::Address
            | EvmBuiltin::Balance
            | EvmBuiltin::Origin
            | EvmBuiltin::Caller
            | EvmBuiltin::CallValue
            | EvmBuiltin::CallDataLoad
            | EvmBuiltin::CallDataSize
            | EvmBuiltin::CallDataCopy
            | EvmBuiltin::CodeSize
            | EvmBuiltin::CodeCopy
            | EvmBuiltin::GasPrice
            | EvmBuiltin::ExtCodeSize
            | EvmBuiltin::ExtCodeCopy
            | EvmBuiltin::ReturnDataSize
            | EvmBuiltin::ReturnDataCopy
            | EvmBuiltin::ExtCodeHash
            | EvmBuiltin::Gas
            | EvmBuiltin::BlockHash
            | EvmBuiltin::Coinbase
            | EvmBuiltin::Timestamp
            | EvmBuiltin::Number
            | EvmBuiltin::Difficulty
            | EvmBuiltin::GasLimit
            | EvmBuiltin::ChainId
            | EvmBuiltin::SelfBalance
            | EvmBuiltin::BaseFee
            | EvmBuiltin::BlobHash
            | EvmBuiltin::BlobBaseFee
            | EvmBuiltin::SLoad
            | EvmBuiltin::SStore
            | EvmBuiltin::TLoad
            | EvmBuiltin::TStore
            | EvmBuiltin::Log0
            | EvmBuiltin::Log1
            | EvmBuiltin::Log2
            | EvmBuiltin::Log3
            | EvmBuiltin::Log4
            | EvmBuiltin::Create
            | EvmBuiltin::Create2
            | EvmBuiltin::Call
            | EvmBuiltin::CallCode
            | EvmBuiltin::DelegateCall
            | EvmBuiltin::StaticCall
            | EvmBuiltin::Return
            | EvmBuiltin::Stop
            | EvmBuiltin::Revert
            | EvmBuiltin::Invalid
            | EvmBuiltin::SelfDestruct
            | EvmBuiltin::DynamicAllocZeroed
            | EvmBuiltin::DynamicAllocAnyBytes
            | EvmBuiltin::MemoryCopy
            | EvmBuiltin::MLoad1
            | EvmBuiltin::MLoad2
            | EvmBuiltin::MLoad3
            | EvmBuiltin::MLoad4
            | EvmBuiltin::MLoad5
            | EvmBuiltin::MLoad6
            | EvmBuiltin::MLoad7
            | EvmBuiltin::MLoad8
            | EvmBuiltin::MLoad9
            | EvmBuiltin::MLoad10
            | EvmBuiltin::MLoad11
            | EvmBuiltin::MLoad12
            | EvmBuiltin::MLoad13
            | EvmBuiltin::MLoad14
            | EvmBuiltin::MLoad15
            | EvmBuiltin::MLoad16
            | EvmBuiltin::MLoad17
            | EvmBuiltin::MLoad18
            | EvmBuiltin::MLoad19
            | EvmBuiltin::MLoad20
            | EvmBuiltin::MLoad21
            | EvmBuiltin::MLoad22
            | EvmBuiltin::MLoad23
            | EvmBuiltin::MLoad24
            | EvmBuiltin::MLoad25
            | EvmBuiltin::MLoad26
            | EvmBuiltin::MLoad27
            | EvmBuiltin::MLoad28
            | EvmBuiltin::MLoad29
            | EvmBuiltin::MLoad30
            | EvmBuiltin::MLoad31
            | EvmBuiltin::MLoad32
            | EvmBuiltin::MStore1
            | EvmBuiltin::MStore2
            | EvmBuiltin::MStore3
            | EvmBuiltin::MStore4
            | EvmBuiltin::MStore5
            | EvmBuiltin::MStore6
            | EvmBuiltin::MStore7
            | EvmBuiltin::MStore8
            | EvmBuiltin::MStore9
            | EvmBuiltin::MStore10
            | EvmBuiltin::MStore11
            | EvmBuiltin::MStore12
            | EvmBuiltin::MStore13
            | EvmBuiltin::MStore14
            | EvmBuiltin::MStore15
            | EvmBuiltin::MStore16
            | EvmBuiltin::MStore17
            | EvmBuiltin::MStore18
            | EvmBuiltin::MStore19
            | EvmBuiltin::MStore20
            | EvmBuiltin::MStore21
            | EvmBuiltin::MStore22
            | EvmBuiltin::MStore23
            | EvmBuiltin::MStore24
            | EvmBuiltin::MStore25
            | EvmBuiltin::MStore26
            | EvmBuiltin::MStore27
            | EvmBuiltin::MStore28
            | EvmBuiltin::MStore29
            | EvmBuiltin::MStore30
            | EvmBuiltin::MStore31
            | EvmBuiltin::MStore32
            | EvmBuiltin::RuntimeStartOffset
            | EvmBuiltin::InitEndOffset
            | EvmBuiltin::RuntimeLength => {
                eval.emit_unsupported_eval_of_evm_builtin(builtin, loc);
                ValueId::ERROR
            }
            _ if !arg_types_valid => ValueId::ERROR,
            EvmBuiltin::Add => self.eval_u256_binop(eval, arg_locals, plank_evm::add),
            EvmBuiltin::Mul => self.eval_u256_binop(eval, arg_locals, plank_evm::mul),
            EvmBuiltin::Sub => self.eval_u256_binop(eval, arg_locals, plank_evm::sub),
            EvmBuiltin::Div => self.eval_u256_binop(eval, arg_locals, plank_evm::div),
            EvmBuiltin::SDiv => self.eval_u256_binop(eval, arg_locals, plank_evm::sdiv),
            EvmBuiltin::Mod => self.eval_u256_binop(eval, arg_locals, plank_evm::r#mod),
            EvmBuiltin::SMod => self.eval_u256_binop(eval, arg_locals, plank_evm::smod),
            EvmBuiltin::Exp => self.eval_u256_binop(eval, arg_locals, plank_evm::exp),
            EvmBuiltin::SignExtend => self.eval_u256_binop(eval, arg_locals, plank_evm::signextend),
            EvmBuiltin::And => self.eval_u256_binop(eval, arg_locals, plank_evm::and),
            EvmBuiltin::Or => self.eval_u256_binop(eval, arg_locals, plank_evm::or),
            EvmBuiltin::Xor => self.eval_u256_binop(eval, arg_locals, plank_evm::xor),
            EvmBuiltin::Byte => self.eval_u256_binop(eval, arg_locals, plank_evm::byte),
            EvmBuiltin::Shl => self.eval_u256_binop(eval, arg_locals, plank_evm::shl),
            EvmBuiltin::Shr => self.eval_u256_binop(eval, arg_locals, plank_evm::shr),
            EvmBuiltin::Sar => self.eval_u256_binop(eval, arg_locals, plank_evm::sar),
            EvmBuiltin::AddMod => {
                self.eval_u256_op(eval, arg_locals, |[a, b, n]| plank_evm::addmod(a, b, n))
            }
            EvmBuiltin::MulMod => {
                self.eval_u256_op(eval, arg_locals, |[a, b, n]| plank_evm::mulmod(a, b, n))
            }
            EvmBuiltin::Lt => self.eval_u256_binop(eval, arg_locals, plank_evm::lt),
            EvmBuiltin::Gt => self.eval_u256_binop(eval, arg_locals, plank_evm::gt),
            EvmBuiltin::SLt => self.eval_u256_binop(eval, arg_locals, plank_evm::slt),
            EvmBuiltin::SGt => self.eval_u256_binop(eval, arg_locals, plank_evm::sgt),
            EvmBuiltin::Eq => self.eval_u256_binop(eval, arg_locals, plank_evm::eq),
            EvmBuiltin::IsZero => self.eval_u256_op(eval, arg_locals, |[a]| plank_evm::iszero(a)),
            EvmBuiltin::Not => self.eval_u256_op(eval, arg_locals, |[a]| plank_evm::not(a)),
        }
    }

    fn eval_u256_binop<R: InternableValue>(
        &self,
        eval: &mut Evaluator<'_>,
        locals: &[hir::LocalId],
        op: impl FnOnce(U256, U256) -> R,
    ) -> ValueId {
        self.eval_u256_op(eval, locals, |[a, b]| op(a, b))
    }

    fn eval_u256_op<R: InternableValue, const N: usize>(
        &self,
        eval: &mut Evaluator<'_>,
        locals: &[hir::LocalId],
        op: impl FnOnce([U256; N]) -> R,
    ) -> ValueId {
        let mut args = [uint!(0U256); N];
        for i in 0..N {
            let (vid, _loc) = self.bindings[locals[i]];
            args[i] = match eval.values.lookup(vid) {
                Value::BigNum(id) => eval.big_nums[id],
                non_num => unreachable!("unexpected non-num value post sig validation {non_num:?}"),
            };
        }
        op(args).intern(eval)
    }
}
