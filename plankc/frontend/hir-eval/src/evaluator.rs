use plank_core::{DenseIndexMap, IndexVec, list_of_lists::ListOfLists, newtype_index};
use plank_hir::{self as hir, ConstId, Hir};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, StrId};
use plank_values::{DefOrigin, Field, Type, TypeId, TypeInterner, Value, ValueId, ValueInterner};

use crate::{
    diagnostics::DiagCtx,
    functions::{EvaluatedFunctionCache, LoweredFunctionsCache},
    operators::OperatorTable,
    scope::{Diverge, EvalContext, LocalState, Scope},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum State<T> {
    InProgress,
    Done(T),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CachedComptimeValue {
    pub value: ValueId,
    pub branches_consumed: u32,
    pub max_eval_branch_quota: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ComptimeQuotaRecord {
    pub branches_consumed: u32,
    pub max_eval_branch_quota: u32,
}

pub(crate) const DEFAULT_COMPTIME_BRANCH_QUOTA: u32 = 1000;

#[derive(Debug, Clone)]
pub(crate) struct ComptimeQuota {
    limit: u32,
    spent: u32,
    depth: u32,
    records: Vec<ComptimeQuotaRecord>,
}

impl Default for ComptimeQuota {
    fn default() -> Self {
        Self { limit: DEFAULT_COMPTIME_BRANCH_QUOTA, spent: 0, depth: 0, records: Vec::new() }
    }
}

impl ComptimeQuota {
    fn reset_budget(&mut self) {
        self.limit = DEFAULT_COMPTIME_BRANCH_QUOTA;
        self.spent = 0;
    }

    pub(crate) fn enter_unit(&mut self) {
        if self.depth == 0 {
            self.reset_budget();
        }
        self.depth = self.depth.checked_add(1).expect("comptime quota depth overflow");
    }

    pub(crate) fn exit_unit(&mut self) {
        self.depth = self.depth.checked_sub(1).expect("comptime quota depth underflow");
    }

    pub(crate) fn raise_limit(&mut self, limit: u32) {
        self.limit = self.limit.max(limit);
        if let Some(record) = self.records.last_mut() {
            record.max_eval_branch_quota = record.max_eval_branch_quota.max(limit);
        }
    }

    pub(crate) fn spend_branch(&mut self) -> bool {
        debug_assert!(self.spent <= self.limit, "comptime quota overspent");
        if self.spent == self.limit {
            return false;
        }
        self.spent += 1;
        if let Some(record) = self.records.last_mut() {
            record.branches_consumed += 1;
        }
        true
    }

    pub(crate) fn limit(&self) -> u32 {
        self.limit
    }

    pub(crate) fn replay_cached(&mut self, cached: CachedComptimeValue) -> bool {
        if self.spent.checked_add(cached.branches_consumed).is_none_or(|spent| spent > self.limit) {
            return false;
        }
        self.spent += cached.branches_consumed;

        if let Some(record) = self.records.last_mut() {
            record.branches_consumed += cached.branches_consumed;
        }
        self.raise_limit(cached.max_eval_branch_quota);
        true
    }

    pub(crate) fn begin_recording(&mut self) {
        self.records.push(ComptimeQuotaRecord::default());
    }

    pub(crate) fn finish_recording(&mut self) -> ComptimeQuotaRecord {
        let record = self.records.pop().expect("comptime quota recording stack underflow");
        if let Some(parent) = self.records.last_mut() {
            parent.branches_consumed += record.branches_consumed;
            parent.max_eval_branch_quota =
                parent.max_eval_branch_quota.max(record.max_eval_branch_quota);
        }
        record
    }

    pub(crate) fn discard_recording(&mut self) {
        self.records.pop().expect("comptime quota recording stack underflow");
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

    pub evaluated_consts: DenseIndexMap<ConstId, State<MaybePoisoned<CachedComptimeValue>>>,
    pub values: &'a mut ValueInterner,
    pub hir: &'a Hir,

    pub evaluated_fns_cache: &'a EvaluatedFunctionCache,
    pub lowered_fns_cache: LoweredFunctionsCache,

    pub call_arg_spans: ListOfLists<CallArgSpansIdx, SourceSpan>,

    pub operator_table: OperatorTable,
    pub(crate) comptime_quota: ComptimeQuota,

    pub instr_stack_buf: Vec<mir::Instruction>,
    pub types_buf: Vec<TypeId>,
    pub locals_buf: Vec<mir::LocalId>,
    pub values_buf: Vec<ValueId>,
    pub maybe_values_buf: Vec<MaybePoisoned<ValueId>>,
    pub fields_buf: Vec<Field>,
    pub captures_buf: Vec<(ValueId, DefOrigin)>,
}

impl<'a> Evaluator<'a> {
    pub fn new(
        hir: &'a Hir,
        types: &'a TypeInterner,
        evaluated_fns_cache: &'a EvaluatedFunctionCache,
        values: &'a mut ValueInterner,
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
            comptime_quota: ComptimeQuota::default(),

            instr_stack_buf: Vec::new(),
            types_buf: Vec::new(),
            locals_buf: Vec::new(),
            values_buf: Vec::new(),
            maybe_values_buf: Vec::new(),
            fields_buf: Vec::new(),
            captures_buf: Vec::new(),
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
    ) -> Result<MaybePoisoned<ValueId>, Diverge> {
        let const_def = self.hir.consts[const_id];
        let mut existing_cached_value = None;
        if let Some(state) = self.evaluated_consts.get(const_id) {
            match state {
                State::Done(Ok(cached)) if self.comptime_quota.replay_cached(*cached) => {
                    return Ok(Ok(cached.value));
                }
                State::Done(Ok(cached)) => existing_cached_value = Some(*cached),
                State::Done(Err(Poisoned)) => return Ok(Err(Poisoned)),
                State::InProgress => {
                    diag_ctx.emit_const_cycle(const_def.name, const_def.loc());
                    self.evaluated_consts[const_id] = State::Done(Err(Poisoned));
                    return Ok(Err(Poisoned));
                }
            }
        }
        if existing_cached_value.is_some() {
            self.evaluated_consts.remove(const_id);
        }

        self.evaluated_consts.insert_no_prev(const_id, State::InProgress);
        self.comptime_quota.begin_recording();

        let mut scope = Scope::new(self, diag_ctx, const_def.source_id, true, EvalContext::Other);
        match scope.eval_comptime(const_def.body) {
            Err(Diverge::ComptimeQuotaExhausted) => {
                scope.eval.comptime_quota.discard_recording();
                match existing_cached_value {
                    Some(cached) => {
                        self.evaluated_consts[const_id] = State::Done(Ok(cached));
                    }
                    None => {
                        self.evaluated_consts.remove(const_id);
                    }
                }
                return Err(Diverge::ComptimeQuotaExhausted);
            }
            Err(Diverge::ControlFlowPoisoned | Diverge::BlockEnd(_)) => {
                scope.eval.comptime_quota.discard_recording();
                self.evaluated_consts[const_id] = State::Done(Err(Poisoned));
                return Ok(Err(Poisoned));
            }
            Ok(_) => {}
        }

        let value = scope.bindings[const_def.result].state.map(|state| match state {
            LocalState::Comptime(vid) => vid,
            LocalState::Runtime(_) => {
                unreachable!("local in comptime set to runtime instead of poisoned")
            }
        });
        if let Some(cached) = existing_cached_value {
            let value = value.expect("cached const re-evaluation should not poison");
            debug_assert_eq!(
                cached.value, value,
                "re-evaluated const produced different cached value"
            );
        }
        let cached_value = match value {
            Ok(value) => {
                let record = scope.eval.comptime_quota.finish_recording();
                Ok(CachedComptimeValue {
                    value,
                    branches_consumed: record.branches_consumed,
                    max_eval_branch_quota: record.max_eval_branch_quota,
                })
            }
            Err(Poisoned) => {
                scope.eval.comptime_quota.discard_recording();
                Err(Poisoned)
            }
        };
        match self.evaluated_consts.get_mut(const_id) {
            Some(State::Done(Err(Poisoned))) => {
                // Already poisoned, don't update
            }
            Some(state @ State::InProgress) => {
                *state = State::Done(cached_value);
                self.try_name_type(const_def.name, value);
            }
            None | Some(State::Done(Ok(_))) => {
                unreachable!("invariant: unset / set to value while evaluating")
            }
        }

        Ok(value)
    }

    fn try_name_type(&mut self, name: StrId, value: MaybePoisoned<ValueId>) {
        let Ok(Value::Type(ty)) = value.map(|vid| self.values.lookup(vid)) else {
            return;
        };
        let Type::Struct(r#struct) = self.types.lookup(ty) else {
            return;
        };
        if r#struct.name.get().is_none() {
            r#struct.name.set(Some(name));
        }
    }

    pub fn lower_entrypoint(
        &mut self,
        block: hir::BlockId,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> mir::FnId {
        let mut scope =
            Scope::new(self, diag_ctx, self.hir.entry_source, false, EvalContext::Other);

        let body = scope.eval_entry_point_body(block);

        let fn_id1 = scope.eval.mir_fn_locals.push_copy_slice(&scope.mir_types);
        let fn_id2 =
            self.mir_fns.push(mir::FnDef { body, param_count: 0, return_type: TypeId::NEVER });
        assert_eq!(fn_id1, fn_id2);

        fn_id1
    }
}
