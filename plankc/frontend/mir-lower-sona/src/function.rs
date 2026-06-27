use crate::module::{DataGlobals, RuntimeShapes, SectionContext, runtime_shape};
use plank_core::{DenseIndexMap, Idx};
use plank_mir::{self as mir, Expr, Instruction, Mir};
use plank_session::RuntimeBuiltin;
use plank_values::{Type as PlankType, TypeId, Value, ValueId, ValueInterner};
use smallvec::SmallVec;
use sonatina_ir::{
    BlockId, HasInst, I256, Immediate, Inst, Type as SonaType, ValueId as SonaValueId,
    builder::{FunctionBuilder, ModuleBuilder, Variable},
    func_cursor::InstInserter,
    inst::{
        arith::{Add, Mul, Sar, Shl, Shr, Sub},
        cast::PtrToInt,
        cmp::{Eq, Gt, IsZero, Lt, Ne, Sgt, Slt},
        control_flow::{Br, Jump},
        data::{ExtractValue, InsertValue, Mload, Mstore, SymAddr, SymSize, SymbolRef},
        evm::{inst_set::EvmInstSet, *},
        logic::{And, Not, Or, Xor},
    },
    module::FuncRef,
    object::EmbedSymbol,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockExit {
    Loose,
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinOutput {
    Value(SonaValueId),
    NoValue,
    Terminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputKind {
    Value,
    NoValue,
    Terminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryOp {
    Load(u16),
    Store(u16),
}

fn memory_op(b: RuntimeBuiltin) -> Option<MemoryOp> {
    use RuntimeBuiltin as B;
    macro_rules! m {
        ($(($l:ident, $s:ident, $n:literal)),* $(,)?) => {
            match b {
                $(B::$l => Some(MemoryOp::Load($n)),)*
                $(B::$s => Some(MemoryOp::Store($n)),)*
                _ => None,
            }
        };
    }
    m! {
        (MLoad1, MStore1, 1),    (MLoad2, MStore2, 2),    (MLoad3, MStore3, 3),
        (MLoad4, MStore4, 4),    (MLoad5, MStore5, 5),    (MLoad6, MStore6, 6),
        (MLoad7, MStore7, 7),    (MLoad8, MStore8, 8),    (MLoad9, MStore9, 9),
        (MLoad10, MStore10, 10), (MLoad11, MStore11, 11), (MLoad12, MStore12, 12),
        (MLoad13, MStore13, 13), (MLoad14, MStore14, 14), (MLoad15, MStore15, 15),
        (MLoad16, MStore16, 16), (MLoad17, MStore17, 17), (MLoad18, MStore18, 18),
        (MLoad19, MStore19, 19), (MLoad20, MStore20, 20), (MLoad21, MStore21, 21),
        (MLoad22, MStore22, 22), (MLoad23, MStore23, 23), (MLoad24, MStore24, 24),
        (MLoad25, MStore25, 25), (MLoad26, MStore26, 26), (MLoad27, MStore27, 27),
        (MLoad28, MStore28, 28), (MLoad29, MStore29, 29), (MLoad30, MStore30, 30),
        (MLoad31, MStore31, 31), (MLoad32, MStore32, 32),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LoweringContext<'a> {
    pub(crate) funcs: &'a DenseIndexMap<mir::FnId, FuncRef>,
    pub(crate) runtime_shapes: &'a RuntimeShapes,
    pub(crate) section_context: SectionContext,
}

pub(crate) struct FunctionLowerer<'a> {
    mir: &'a Mir,
    values: &'a ValueInterner,
    fn_id: mir::FnId,
    fb: FunctionBuilder<InstInserter>,
    is: &'static EvmInstSet,
    context: LoweringContext<'a>,
    local_vars: DenseIndexMap<mir::LocalId, Option<(Variable, SonaType)>>,
    data_globals: &'a DataGlobals,
}

impl<'a> FunctionLowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        builder: &ModuleBuilder,
        is: &'static EvmInstSet,
        mir: &'a Mir,
        values: &'a ValueInterner,
        data_globals: &'a DataGlobals,
        fn_id: mir::FnId,
        context: LoweringContext<'a>,
    ) -> Self {
        let mut fb = builder.func_builder::<InstInserter>(context.funcs[fn_id]);
        let entry = fb.append_block();
        fb.switch_to_block(entry);
        Self {
            mir,
            values,
            fn_id,
            fb,
            is,
            context,
            local_vars: DenseIndexMap::with_capacity(mir.fn_locals[fn_id].len()),
            data_globals,
        }
    }

    pub(crate) fn lower(mut self) {
        for (idx, &ty) in self.mir.fn_locals[self.fn_id].iter().enumerate() {
            let local = mir::LocalId::new(idx as u32);
            let slot = self.shape(ty).map(|ty| (self.fb.declare_var(ty), ty));
            self.local_vars.insert_no_prev(local, slot);
        }

        let mut arg_idx = 0;
        for param in self.mir.fns[self.fn_id].iter_params() {
            if let Some((var, _)) = self.local_slot(param) {
                let arg = self.fb.args()[arg_idx];
                self.fb.def_var(var, arg);
                arg_idx += 1;
            }
        }
        assert_eq!(arg_idx, self.fb.args().len());

        if self.lower_block(self.mir.fns[self.fn_id].body) == BlockExit::Loose {
            self.fb.insert_inst_no_result(EvmInvalid::new(self.is));
        }
        self.fb.seal_all();
        self.fb.finish();
    }

    fn lower_block(&mut self, block: mir::BlockId) -> BlockExit {
        for &instr in &self.mir.blocks[block] {
            let exit = match instr {
                Instruction::Set { target, expr } => self.lower_set(target, expr),
                Instruction::Return(local) => {
                    let value = self.read_local(local);
                    self.fb.insert_return_values(value.as_slice());
                    return BlockExit::Terminated;
                }
                Instruction::If { condition, then_block, else_block } => {
                    self.lower_if(condition, then_block, else_block)
                }
                Instruction::While { condition_block, condition, body } => {
                    self.lower_while(condition_block, condition, body)
                }
            };
            if exit == BlockExit::Terminated {
                return BlockExit::Terminated;
            }
        }
        BlockExit::Loose
    }

    fn lower_set(&mut self, target: mir::LocalId, expr: Expr) -> BlockExit {
        match expr {
            Expr::Const(value) => {
                let value = self.materialize_constant(value);
                self.write_local(target, value);
            }
            Expr::LocalRef(src) => {
                let value = self.read_local(src);
                self.write_local(target, value);
            }
            Expr::RuntimeBuiltinCall { builtin, args } => {
                let result_ty = self.shape(self.mir.fn_locals[self.fn_id][target.idx()]);
                assert!(
                    !result_ty.is_some_and(SonaType::is_compound),
                    "{builtin} cannot return an aggregate value"
                );
                match self.lower_builtin(builtin, args, result_ty) {
                    BuiltinOutput::Value(value) => self.write_local(target, Some(value)),
                    BuiltinOutput::NoValue => self.write_local(target, None),
                    BuiltinOutput::Terminator => return BlockExit::Terminated,
                }
            }
            Expr::Call { callee, args } => {
                let call_args = self.mir.args[args]
                    .iter()
                    .filter_map(|&arg| self.read_local(arg))
                    .collect::<SmallVec<[SonaValueId; 8]>>();
                let results = self.fb.insert_call_results(self.context.funcs[callee], call_args);
                let value = match self.local_slot(target) {
                    None => {
                        assert!(results.is_empty());
                        None
                    }
                    Some(_) => {
                        assert_eq!(results.len(), 1);
                        Some(results[0])
                    }
                };
                self.write_local(target, value);
                if self.mir.fns[callee].return_type == TypeId::NEVER {
                    self.fb.insert_inst_no_result(EvmInvalid::new(self.is));
                    return BlockExit::Terminated;
                }
            }
            Expr::CompoundLit { ty, fields } => {
                let value = self.build_aggregate(ty, self.mir.args[fields].len(), |this, i| {
                    this.read_local(this.mir.args[fields][i])
                });
                self.write_local(target, value);
            }
            Expr::FieldAccess { object, field_index } => {
                let value = self.read_field(object, field_index);
                self.write_local(target, value);
            }
            Expr::DataOffset { contents, start } => {
                let gv = self.data_globals[&contents];
                let symbol = SymbolRef::Global(gv);
                let mut value = self.i256_inst(SymAddr::new(self.is, symbol));
                if start != 0 {
                    let start = self.imm_256(start);
                    value = self.i256_inst(Add::new(self.is, value, start));
                }
                self.write_local(target, Some(value));
            }
        }
        BlockExit::Loose
    }

    fn lower_if(
        &mut self,
        condition: mir::LocalId,
        then_block: mir::BlockId,
        else_block: mir::BlockId,
    ) -> BlockExit {
        let else_ = self.fb.append_block();
        let then_ = self.fb.append_block();
        let condition = self.read_condition(condition);
        self.br(condition, then_, else_);

        self.fb.switch_to_block(then_);
        let then_exit = self.lower_block(then_block);
        let then_end = (then_exit == BlockExit::Loose)
            .then(|| self.fb.current_block().expect("loose then-branch must have a current block"));

        self.fb.switch_to_block(else_);
        let else_exit = self.lower_block(else_block);
        let else_end = (else_exit == BlockExit::Loose)
            .then(|| self.fb.current_block().expect("loose else-branch must have a current block"));

        if then_end.is_none() && else_end.is_none() {
            return BlockExit::Terminated;
        }

        let merge = self.fb.append_block();
        if let Some(then_end) = then_end {
            self.fb.switch_to_block(then_end);
            self.jump(merge);
        }
        if let Some(else_end) = else_end {
            self.fb.switch_to_block(else_end);
            self.jump(merge);
        }

        self.fb.switch_to_block(merge);
        BlockExit::Loose
    }

    fn lower_while(
        &mut self,
        condition_block: mir::BlockId,
        condition: mir::LocalId,
        body: mir::BlockId,
    ) -> BlockExit {
        let condition_sona = self.fb.append_block();
        self.jump(condition_sona);

        self.fb.switch_to_block(condition_sona);
        if self.lower_block(condition_block) == BlockExit::Terminated {
            return BlockExit::Terminated;
        }

        let body_sona = self.fb.append_block();
        let continue_sona = self.fb.append_block();
        let condition = self.read_condition(condition);
        self.br(condition, body_sona, continue_sona);

        self.fb.switch_to_block(body_sona);
        if self.lower_block(body) == BlockExit::Loose {
            self.jump(condition_sona);
        }

        self.fb.switch_to_block(continue_sona);
        BlockExit::Loose
    }

    fn local_slot(&self, local: mir::LocalId) -> Option<(Variable, SonaType)> {
        self.local_vars[local]
    }

    fn read_local(&mut self, local: mir::LocalId) -> Option<SonaValueId> {
        self.local_slot(local).map(|(var, _)| self.fb.use_var(var))
    }

    fn read_value(&mut self, local: mir::LocalId) -> SonaValueId {
        self.read_local(local).expect("local has no runtime value")
    }

    fn write_local(&mut self, local: mir::LocalId, value: Option<SonaValueId>) {
        match (self.local_slot(local), value) {
            (None, None) => {}
            (Some((var, _)), Some(value)) => self.fb.def_var(var, value),
            (None, Some(_)) => panic!("zero-sized local {local:?} got a value"),
            (Some(_), None) => panic!("valued local {local:?} got no value"),
        }
    }

    fn shape(&self, ty: TypeId) -> Option<SonaType> {
        runtime_shape(self.context.runtime_shapes, ty)
    }

    fn read_condition(&mut self, local: mir::LocalId) -> SonaValueId {
        let condition = self.read_value(local);
        let condition_ty = self.fb.type_of(condition);
        if condition_ty == SonaType::I1 {
            condition
        } else {
            let zero = self.fb.make_imm_value(Immediate::from_i256(I256::from(0), condition_ty));
            self.fb.insert_inst(Ne::new(self.is, condition, zero), SonaType::I1)
        }
    }

    fn build_aggregate(
        &mut self,
        ty: TypeId,
        element_count: usize,
        mut get_element: impl FnMut(&mut Self, usize) -> Option<SonaValueId>,
    ) -> Option<SonaValueId> {
        let expected_count = match self.mir.types.lookup(ty) {
            PlankType::Compound(compound) => compound.field_count(),
            PlankType::Primitive(_) => panic!("aggregate on primitive"),
        };
        assert_eq!(expected_count, element_count);
        let struct_ty = self.shape(ty)?;
        let mut aggregate = self.fb.make_undef_value(struct_ty);
        for i in 0..element_count {
            if let Some(element_value) = get_element(self, i) {
                let i = u32::try_from(i).expect("aggregate element index must fit in u32");
                let idx = self.imm_256(i);
                aggregate = self.fb.insert_inst(
                    InsertValue::new_unchecked(self.is, aggregate, idx, element_value),
                    struct_ty,
                );
            }
        }
        Some(aggregate)
    }

    fn read_field(&mut self, object: mir::LocalId, field_index: u32) -> Option<SonaValueId> {
        let object_type = self.mir.fn_locals[self.fn_id][object.idx()];
        let PlankType::Compound(compound) = self.mir.types.lookup(object_type) else {
            panic!("field access on non-compound");
        };
        let field_ty = self.shape(compound.field_type(field_index as usize))?;
        let object_value = self.read_value(object);
        let idx = self.imm_256(field_index);
        Some(self.fb.insert_inst(ExtractValue::new_unchecked(self.is, object_value, idx), field_ty))
    }

    fn materialize_constant(&mut self, value: ValueId) -> Option<SonaValueId> {
        match self.values.lookup(value) {
            Value::Bool(value) => Some(self.fb.make_imm_value(Immediate::I1(value))),
            Value::BigNum(value) => {
                let bytes = value.to_be_bytes::<32>();
                let value = sonatina_ir::U256::from_big_endian(&bytes);
                Some(
                    self.fb.make_imm_value(Immediate::from_i256(I256::from(value), SonaType::I256)),
                )
            }
            Value::Compound { ty, fields } => self
                .build_aggregate(ty, fields.len(), |this, i| this.materialize_constant(fields[i])),
            Value::Type(_) | Value::Bytes(_) | Value::Closure { .. } => {
                panic!("comptime-only value in MIR")
            }
        }
    }

    fn imm_256(&mut self, n: u32) -> SonaValueId {
        self.fb.make_imm_value(Immediate::from_i256(I256::from(n), SonaType::I256))
    }

    fn jump(&mut self, target: BlockId) {
        self.fb.insert_inst_no_result(Jump::new(self.is, target));
    }

    fn br(&mut self, cond: SonaValueId, then: BlockId, else_: BlockId) {
        self.fb.insert_inst_no_result(Br::new(self.is, cond, then, else_));
    }

    fn lower_builtin(
        &mut self,
        op: RuntimeBuiltin,
        args: mir::ArgsId,
        rty: Option<SonaType>,
    ) -> BuiltinOutput {
        use OutputKind::*;
        use RuntimeBuiltin as B;

        macro_rules! emit {
            ($make:path, [$($a:ident),* $(,)?], $kind:expr) => {
                self.emit(op, args, rty, $kind, |is, [$($a),*]| $make(is, $($a),*))
            };
        }
        match op {
            B::Add => emit!(Add::new, [a, b], Value),
            B::Mul => emit!(Mul::new, [a, b], Value),
            B::Sub => emit!(Sub::new, [a, b], Value),
            B::Div => emit!(EvmUdiv::new, [a, b], Value),
            B::SDiv => emit!(EvmSdiv::new, [a, b], Value),
            B::Mod => emit!(EvmUmod::new, [a, b], Value),
            B::SMod => emit!(EvmSmod::new, [a, b], Value),
            B::AddMod => emit!(EvmAddMod::new, [a, b, c], Value),
            B::MulMod => emit!(EvmMulMod::new, [a, b, c], Value),
            B::Exp => emit!(EvmExp::new, [a, b], Value),
            B::SignExtend => emit!(EvmSignExtend::new, [a, b], Value),

            B::Lt => emit!(Lt::new, [a, b], Value),
            B::Gt => emit!(Gt::new, [a, b], Value),
            B::SLt => emit!(Slt::new, [a, b], Value),
            B::SGt => emit!(Sgt::new, [a, b], Value),
            B::Eq => emit!(Eq::new, [a, b], Value),
            B::IsZero => emit!(IsZero::new, [a], Value),
            B::And => emit!(And::new, [a, b], Value),
            B::Or => emit!(Or::new, [a, b], Value),
            B::Xor => emit!(Xor::new, [a, b], Value),
            B::Not => emit!(Not::new, [a], Value),
            B::Byte => emit!(EvmByte::new, [a, b], Value),
            B::Shl => emit!(Shl::new, [a, b], Value),
            B::Shr => emit!(Shr::new, [a, b], Value),
            B::Sar => emit!(Sar::new, [a, b], Value),
            B::Clz => emit!(EvmClz::new, [a], Value),

            B::Keccak256 => emit!(EvmKeccak256::new, [a, b], Value),
            B::Address => emit!(EvmAddress::new, [], Value),
            B::Balance => emit!(EvmBalance::new, [a], Value),
            B::Origin => emit!(EvmOrigin::new, [], Value),
            B::Caller => emit!(EvmCaller::new, [], Value),
            B::CallValue => emit!(EvmCallValue::new, [], Value),
            B::CallDataLoad => emit!(EvmCalldataLoad::new, [a], Value),
            B::CallDataSize => emit!(EvmCalldataSize::new, [], Value),
            B::CallDataCopy => emit!(EvmCalldataCopy::new, [a, b, c], NoValue),
            B::CodeSize => emit!(EvmCodeSize::new, [], Value),
            B::CodeCopy => emit!(EvmCodeCopy::new, [a, b, c], NoValue),
            B::GasPrice => emit!(EvmGasPrice::new, [], Value),
            B::ExtCodeSize => emit!(EvmExtCodeSize::new, [a], Value),
            B::ExtCodeCopy => emit!(EvmExtCodeCopy::new, [a, b, c, d], NoValue),
            B::ReturnDataSize => emit!(EvmReturnDataSize::new, [], Value),
            B::ReturnDataCopy => emit!(EvmReturnDataCopy::new, [a, b, c], NoValue),
            B::ExtCodeHash => emit!(EvmExtCodeHash::new, [a], Value),
            B::Gas => emit!(EvmGas::new, [], Value),

            B::BlockHash => emit!(EvmBlockHash::new, [a], Value),
            B::Coinbase => emit!(EvmCoinBase::new, [], Value),
            B::Timestamp => emit!(EvmTimestamp::new, [], Value),
            B::Number => emit!(EvmNumber::new, [], Value),
            B::Difficulty => emit!(EvmPrevRandao::new, [], Value),
            B::GasLimit => emit!(EvmGasLimit::new, [], Value),
            B::ChainId => emit!(EvmChainId::new, [], Value),
            B::SelfBalance => emit!(EvmSelfBalance::new, [], Value),
            B::BaseFee => emit!(EvmBaseFee::new, [], Value),
            B::BlobHash => emit!(EvmBlobHash::new, [a], Value),
            B::BlobBaseFee => emit!(EvmBlobBaseFee::new, [], Value),

            B::SLoad => emit!(EvmSload::new, [a], Value),
            B::SStore => emit!(EvmSstore::new, [a, b], NoValue),
            B::TLoad => emit!(EvmTload::new, [a], Value),
            B::TStore => emit!(EvmTstore::new, [a, b], NoValue),

            B::Log0 => emit!(EvmLog0::new, [a, b], NoValue),
            B::Log1 => emit!(EvmLog1::new, [a, b, c], NoValue),
            B::Log2 => emit!(EvmLog2::new, [a, b, c, d], NoValue),
            B::Log3 => emit!(EvmLog3::new, [a, b, c, d, e], NoValue),
            B::Log4 => emit!(EvmLog4::new, [a, b, c, d, e, f], NoValue),

            B::Create => emit!(EvmCreate::new, [a, b, c], Value),
            B::Create2 => emit!(EvmCreate2::new, [a, b, c, d], Value),
            B::Call => emit!(EvmCall::new, [a, b, c, d, e, f, g], Value),
            B::CallCode => emit!(EvmCallCode::new, [a, b, c, d, e, f, g], Value),
            B::DelegateCall => emit!(EvmDelegateCall::new, [a, b, c, d, e, f], Value),
            B::StaticCall => emit!(EvmStaticCall::new, [a, b, c, d, e, f], Value),
            B::Return => emit!(EvmReturn::new, [a, b], Terminator),
            B::Stop => emit!(EvmStop::new, [], Terminator),
            B::Revert => emit!(EvmRevert::new, [a, b], Terminator),
            B::Invalid => emit!(EvmInvalid::new, [], Terminator),
            B::SelfDestruct => emit!(EvmSelfDestruct::new, [a], Terminator),

            B::DynamicAllocZeroed => self.alloc(op, args, rty, true),
            B::DynamicAllocAnyBytes => self.alloc(op, args, rty, false),
            B::MemoryCopy => emit!(EvmMcopy::new, [a, b, c], NoValue),

            B::RuntimeStartOffset => {
                let result_ty = rty.expect("builtin should produce a value");
                let result = match (self.context.section_context, self.mir.run.is_some()) {
                    (SectionContext::Init, true) => {
                        let symbol = SymbolRef::Embed(EmbedSymbol::from(crate::RUNTIME_SECTION));
                        self.fb.insert_inst(SymAddr::new(self.is, symbol), result_ty)
                    }
                    (SectionContext::Init, false) => self
                        .fb
                        .insert_inst(SymSize::new(self.is, SymbolRef::CurrentSection), result_ty),
                    (SectionContext::Runtime, _) => self
                        .fb
                        .insert_inst(SymAddr::new(self.is, SymbolRef::CurrentSection), result_ty),
                };
                BuiltinOutput::Value(result)
            }
            B::InitEndOffset => {
                let result_ty = rty.expect("builtin should produce a value");
                BuiltinOutput::Value(
                    self.fb
                        .insert_inst(SymSize::new(self.is, SymbolRef::CurrentSection), result_ty),
                )
            }
            B::RuntimeLength => {
                let result_ty = rty.expect("builtin should produce a value");
                let result = match (self.context.section_context, self.mir.run.is_some()) {
                    (SectionContext::Init, true) => {
                        let symbol = SymbolRef::Embed(EmbedSymbol::from(crate::RUNTIME_SECTION));
                        self.fb.insert_inst(SymSize::new(self.is, symbol), result_ty)
                    }
                    (SectionContext::Init, false) => self.imm_256(0),
                    (SectionContext::Runtime, _) => self
                        .fb
                        .insert_inst(SymSize::new(self.is, SymbolRef::CurrentSection), result_ty),
                };
                BuiltinOutput::Value(result)
            }

            builtin => match memory_op(builtin) {
                Some(MemoryOp::Load(bytes)) => {
                    let [addr] = self.arg_values(builtin, args);
                    let word = self.i256_inst(Mload::new(self.is, addr, SonaType::I256));
                    if bytes == 32 {
                        return BuiltinOutput::Value(word);
                    }
                    let bits = self.imm_256(256 - bytes as u32 * 8);
                    let shifted = self.i256_inst(Shr::new(self.is, bits, word));
                    BuiltinOutput::Value(shifted)
                }
                Some(MemoryOp::Store(bytes)) => {
                    let [addr, value] = self.arg_values(builtin, args);
                    if bytes == 32 {
                        self.fb.insert_inst_no_result(Mstore::new(
                            self.is,
                            addr,
                            value,
                            SonaType::I256,
                        ));
                        return BuiltinOutput::NoValue;
                    }
                    if bytes == 1 {
                        self.fb.insert_inst_no_result(EvmMstore8::new(self.is, addr, value));
                        return BuiltinOutput::NoValue;
                    }

                    let bits = bytes as u32 * 8;
                    let tail_bits = self.imm_256(256 - bits);
                    let bits = self.imm_256(bits);

                    let tail = self.i256_inst(Mload::new(self.is, addr, SonaType::I256));
                    let tail = self.i256_inst(Shl::new(self.is, bits, tail));
                    let tail = self.i256_inst(Shr::new(self.is, bits, tail));
                    let value = self.i256_inst(Shl::new(self.is, tail_bits, value));
                    let updated = self.i256_inst(Or::new(self.is, tail, value));
                    self.fb.insert_inst_no_result(Mstore::new(
                        self.is,
                        addr,
                        updated,
                        SonaType::I256,
                    ));
                    BuiltinOutput::NoValue
                }
                None => panic!("unsupported RuntimeBuiltin"),
            },
        }
    }

    fn emit<I, const N: usize>(
        &mut self,
        builtin: RuntimeBuiltin,
        args: mir::ArgsId,
        result_ty: Option<SonaType>,
        kind: OutputKind,
        make: impl FnOnce(&dyn HasInst<I>, [SonaValueId; N]) -> I,
    ) -> BuiltinOutput
    where
        I: Inst,
        EvmInstSet: HasInst<I>,
    {
        let args = self.arg_values(builtin, args);
        let inst = make(self.is, args);
        match kind {
            OutputKind::Value => BuiltinOutput::Value(
                self.fb.insert_inst(inst, result_ty.expect("builtin should produce a value")),
            ),
            OutputKind::NoValue => {
                self.fb.insert_inst_no_result(inst);
                BuiltinOutput::NoValue
            }
            OutputKind::Terminator => {
                self.fb.insert_inst_no_result(inst);
                BuiltinOutput::Terminator
            }
        }
    }

    fn alloc(
        &mut self,
        builtin: RuntimeBuiltin,
        args: mir::ArgsId,
        result_ty: Option<SonaType>,
        zeroed: bool,
    ) -> BuiltinOutput {
        let [size] = self.arg_values(builtin, args);
        let result_ty = result_ty.expect("builtin should produce a value");
        BuiltinOutput::Value({
            let ptr_ty = self.fb.ptr_type(SonaType::I8);
            let ptr = self.fb.insert_inst(EvmMalloc::new(self.is, size), ptr_ty);
            if zeroed {
                let data_offset = self.i256_inst(EvmCalldataSize::new(self.is));
                self.fb.insert_inst_no_result(EvmCalldataCopy::new(
                    self.is,
                    ptr,
                    data_offset,
                    size,
                ));
            }
            self.fb.insert_inst(PtrToInt::new(self.is, ptr, SonaType::I256), result_ty)
        })
    }

    fn arg_values<const N: usize>(
        &mut self,
        builtin: RuntimeBuiltin,
        args: mir::ArgsId,
    ) -> [SonaValueId; N] {
        let args = &self.mir.args[args];
        assert_eq!(args.len(), N, "{builtin} expects {N} arguments, got {}", args.len());
        let mut values = [SonaValueId(0); N];
        for (value, arg) in values.iter_mut().zip(args) {
            *value = self.read_value(*arg);
        }
        values
    }

    fn i256_inst<I>(&mut self, inst: I) -> SonaValueId
    where
        I: Inst,
        EvmInstSet: HasInst<I>,
    {
        self.fb.insert_inst(inst, SonaType::I256)
    }
}
