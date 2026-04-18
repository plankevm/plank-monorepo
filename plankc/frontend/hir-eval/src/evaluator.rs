use plank_core::{DenseIndexMap, IndexVec, list_of_lists::ListOfLists, newtype_index};
use plank_hir::{self as hir, ConstId, Hir};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, StrId};
use plank_values::{DefOrigin, Field, Type, TypeId, TypeInterner, Value, ValueId, ValueInterner};

use crate::{
    diagnostics::DiagCtx,
    functions::{EvaluatedFunctionCache, LoweredFunctionsCache},
    scope::{Diverge, EvalContext, LocalState, Scope},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum State<T> {
    InProgress,
    Done(T),
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

    pub evaluated_consts: DenseIndexMap<ConstId, State<MaybePoisoned<ValueId>>>,
    pub values: &'a mut ValueInterner,
    pub hir: &'a Hir,

    pub evaluated_fns_cache: &'a EvaluatedFunctionCache,
    pub lowered_fns_cache: LoweredFunctionsCache,

    pub call_arg_spans: ListOfLists<CallArgSpansIdx, SourceSpan>,

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
    ) -> MaybePoisoned<ValueId> {
        let const_def = self.hir.consts[const_id];
        match self.evaluated_consts.get_mut(const_id) {
            Some(State::Done(vid)) => return *vid,
            Some(state @ State::InProgress) => {
                diag_ctx.emit_const_cycle(const_def.name, const_def.loc());
                *state = State::Done(Err(Poisoned));
                return Err(Poisoned);
            }
            None => {}
        };

        self.evaluated_consts.insert_no_prev(const_id, State::InProgress);

        let mut scope = Scope::new(self, diag_ctx, const_def.source_id, true, EvalContext::Other);
        match scope.eval_comptime(const_def.body) {
            Err(Diverge::ControlFlowPoisoned | Diverge::BlockEnd(_)) => {
                self.evaluated_consts[const_id] = State::Done(Err(Poisoned));
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
        match self.evaluated_consts.get_mut(const_id) {
            Some(State::Done(Err(Poisoned))) => {
                // Already poisoned, don't update
            }
            Some(state @ State::InProgress) => {
                *state = State::Done(value);
                self.try_name_type(const_def.name, value);
            }
            None | Some(State::Done(Ok(_))) => {
                unreachable!("invariant: unset / set to value while evaluating")
            }
        }

        value
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
