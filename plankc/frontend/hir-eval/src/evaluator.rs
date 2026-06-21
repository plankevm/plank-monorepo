use plank_core::{
    DenseIndexMap, IndexVec, dense_index_map::Entry, list_of_lists::ListOfLists, newtype_index,
};
use plank_evm::EvmVersion;
use plank_hir::{self as hir, ConstId, Hir};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, SrcLoc, StrId, ZERO_SPAN};
use plank_values::{
    Compound, DefOrigin, Field, Type, TypeId, TypeInterner, TypeName, Value, ValueId, ValueInterner,
};

use crate::{
    diagnostics::DiagCtx,
    functions::{EvaluatedFunctionCache, LoweredFunctionsCache},
    operators::OperatorTable,
    quota::ComptimeQuota,
    scope::{Diverge, EvalContext, LocalState, Scope},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum State<T> {
    InProgress,
    Done(T),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ConstEvalResult {
    Value(ValueId),
    Poisoned,
    QuotaExhausted,
}

impl ConstEvalResult {
    fn value(self) -> MaybePoisoned<ValueId> {
        match self {
            ConstEvalResult::Value(value) => Ok(value),
            ConstEvalResult::Poisoned | ConstEvalResult::QuotaExhausted => Err(Poisoned),
        }
    }
}

newtype_index! {
    pub(crate) struct CallArgSpansIdx;
}

pub(crate) struct Evaluator<'a> {
    pub mir_blocks: ListOfLists<mir::BlockId, mir::Instruction>,
    pub mir_args: ListOfLists<mir::ArgsId, mir::LocalId>,
    pub mir_fns: IndexVec<mir::FnId, mir::FnDef>,
    pub mir_fn_locals: ListOfLists<mir::FnId, TypeId>,
    pub types: &'a TypeInterner,

    pub evaluated_consts: DenseIndexMap<ConstId, State<ConstEvalResult>>,
    pub values: &'a mut ValueInterner,
    pub hir: &'a Hir,

    pub evaluated_fns_cache: &'a EvaluatedFunctionCache,
    pub lowered_fns_cache: LoweredFunctionsCache,

    pub call_arg_spans: ListOfLists<CallArgSpansIdx, SourceSpan>,

    pub operator_table: OperatorTable,

    pub instr_stack_buf: Vec<mir::Instruction>,
    pub types_buf: Vec<TypeId>,
    pub locals_buf: Vec<mir::LocalId>,
    pub values_buf: Vec<ValueId>,
    pub maybe_values_buf: Vec<MaybePoisoned<ValueId>>,
    pub type_name_args_buf: Vec<ValueId>,
    pub fields_buf: Vec<Field>,
    pub captures_buf: Vec<(ValueId, DefOrigin)>,

    pub evm_version: EvmVersion,
}

impl<'a> Evaluator<'a> {
    pub fn new(
        hir: &'a Hir,
        types: &'a TypeInterner,
        evaluated_fns_cache: &'a EvaluatedFunctionCache,
        values: &'a mut ValueInterner,
        evm_version: EvmVersion,
    ) -> Self {
        Evaluator {
            mir_blocks: ListOfLists::new(),
            mir_fns: IndexVec::new(),
            mir_fn_locals: ListOfLists::new(),
            mir_args: ListOfLists::new(),
            types,

            evaluated_consts: DenseIndexMap::new(),
            values,
            hir,

            evaluated_fns_cache,
            lowered_fns_cache: LoweredFunctionsCache::new(),

            call_arg_spans: ListOfLists::new(),

            operator_table: OperatorTable::new(),

            instr_stack_buf: Vec::new(),
            types_buf: Vec::new(),
            locals_buf: Vec::new(),
            values_buf: Vec::new(),
            maybe_values_buf: Vec::new(),
            type_name_args_buf: Vec::new(),
            fields_buf: Vec::new(),
            captures_buf: Vec::new(),

            evm_version,
        }
    }

    pub fn is_comptime_only(&self, value: ValueId) -> bool {
        let ty = self.values.type_of_value(value);
        self.types.is_comptime_only(ty)
    }

    pub fn evaluate_const(
        &mut self,
        const_id: ConstId,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> MaybePoisoned<ValueId> {
        let const_def = self.hir.consts[const_id];
        match self.evaluated_consts.entry(const_id) {
            Entry::Occupied(State::Done(result)) => return result.value(),
            Entry::Occupied(state @ State::InProgress) => {
                diag_ctx.emit_const_cycle(const_def.name, const_def.loc());
                *state = State::Done(ConstEvalResult::Poisoned);
                return Err(Poisoned);
            }
            Entry::Vacant(vacant) => vacant.insert(State::InProgress),
        };

        let mut scope = Scope::new(
            self,
            diag_ctx,
            const_def.source_id,
            true,
            ComptimeQuota::default(),
            const_def.loc(),
            EvalContext::Other,
        );
        match scope.eval_comptime(const_def.body) {
            Err(Diverge::ComptimeQuotaExhausted) => {
                self.evaluated_consts[const_id] = State::Done(ConstEvalResult::QuotaExhausted);
                return Err(Poisoned);
            }
            Err(Diverge::ControlFlowPoisoned | Diverge::BlockEnd(_)) => {
                self.evaluated_consts[const_id] = State::Done(ConstEvalResult::Poisoned);
                return Err(Poisoned);
            }
            Ok(_) => {}
        }

        let value = scope.bindings[const_def.result].state.map(|state| match state {
            LocalState::Comptime(vid) => vid,
            LocalState::Runtime(_) => {
                unreachable!("local in comptime set to runtime instead of poisoned")
            }
        });
        let const_result = match value {
            Ok(value) => ConstEvalResult::Value(value),
            Err(Poisoned) => ConstEvalResult::Poisoned,
        };

        match self.evaluated_consts.get_mut(const_id) {
            Some(State::Done(ConstEvalResult::Poisoned)) => {}
            Some(state @ State::InProgress) => {
                *state = State::Done(const_result);
                self.try_name_type(const_def.name, value);
            }
            None
            | Some(State::Done(ConstEvalResult::Value(_) | ConstEvalResult::QuotaExhausted)) => {
                unreachable!("invariant: const state changed while evaluating")
            }
        }

        value
    }

    fn try_name_type(&mut self, name: StrId, value: MaybePoisoned<ValueId>) {
        let Ok(value) = value else { return };
        match self.values.lookup(value) {
            Value::Type(ty) => {
                let Type::Compound(Compound::Struct(r#struct)) = self.types.lookup(ty) else {
                    return;
                };
                if r#struct.name.get().is_none() {
                    r#struct.name.set(Some(TypeName::Plain(name)));
                }
            }
            Value::Closure { .. } => {
                self.values.try_name_closure(value, name);
            }
            _ => {}
        }
    }

    pub fn lower_entrypoint(
        &mut self,
        entry_point: hir::EntryPoint,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> mir::FnId {
        let eval_branch_quota_start_loc = match self.hir.block_spans[entry_point.body] {
            Ok(span) => SrcLoc::new(entry_point.source_id, span),
            Err(Poisoned) => SrcLoc::new(entry_point.source_id, ZERO_SPAN),
        };
        let mut scope = Scope::new(
            self,
            diag_ctx,
            entry_point.source_id,
            false,
            ComptimeQuota::default(),
            eval_branch_quota_start_loc,
            EvalContext::Other,
        );

        let body = scope.eval_entry_point_body(entry_point.body);

        let fn_id1 = scope.eval.mir_fn_locals.push_copy_slice(&scope.mir_types);
        let fn_id2 =
            self.mir_fns.push(mir::FnDef { body, param_count: 0, return_type: TypeId::NEVER });
        assert_eq!(fn_id1, fn_id2);

        fn_id1
    }
}
