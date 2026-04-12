use hashbrown::HashMap;
use plank_core::{DenseIndexMap, IndexVec, list_of_lists::ListOfLists};
use plank_hir::{self as hir, ConstId, Hir};
use plank_mir as mir;
use plank_session::{MaybePoisoned, StrId};
use plank_values::{DefOrigin, TypeId, TypeInterner, Value, ValueId, ValueInterner};

use crate::{
    diagnostics::DiagCtx,
    scope::{Function, LocalState, Scope},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum State<T> {
    InProgress,
    Done(T),
}

pub(crate) struct Evaluator<'a> {
    // MIR
    pub mir_blocks: ListOfLists<mir::BlockId, mir::Instruction>,
    pub mir_args: ListOfLists<mir::ArgsId, mir::LocalId>,
    pub mir_fns: IndexVec<mir::FnId, mir::FnDef>,
    pub mir_fn_locals: ListOfLists<mir::FnId, TypeId>,
    pub types: TypeInterner,

    pub evaluated_consts: DenseIndexMap<ConstId, State<MaybePoisoned<ValueId>>>,
    pub values: &'a mut ValueInterner,
    pub hir: &'a Hir,

    pub lowered_fns_cache: HashMap<ValueId, State<MaybePoisoned<mir::FnId>>>,

    pub instr_stack_buf: Vec<mir::Instruction>,
    pub types_buf: Vec<TypeId>,
    pub locals_buf: Vec<mir::LocalId>,
    pub values_buf: Vec<ValueId>,
    pub fields_buf: Vec<(StrId, TypeId)>,
    pub captures_buf: Vec<(ValueId, DefOrigin)>,
}

impl<'a> Evaluator<'a> {
    pub fn new(hir: &'a Hir, values: &'a mut ValueInterner) -> Self {
        Evaluator {
            mir_blocks: ListOfLists::new(),
            mir_fns: IndexVec::new(),
            mir_fn_locals: ListOfLists::new(),
            mir_args: ListOfLists::new(),
            types: TypeInterner::new(),

            evaluated_consts: DenseIndexMap::new(),
            values,
            hir,

            lowered_fns_cache: HashMap::new(),

            instr_stack_buf: Vec::new(),
            types_buf: Vec::new(),
            locals_buf: Vec::new(),
            values_buf: Vec::new(),
            fields_buf: Vec::new(),
            captures_buf: Vec::new(),
        }
    }

    pub fn is_comptime_only(&self, value: ValueId) -> bool {
        let ty = self.values.type_of_value(value);
        self.types.comptime_only(ty)
    }

    pub fn evaluate_const(
        &mut self,
        const_id: ConstId,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> MaybePoisoned<ValueId> {
        let const_def = self.hir.consts[const_id];
        match self.evaluated_consts.get(const_id) {
            Some(&State::Done(vid)) => return vid,
            Some(State::InProgress) => {
                diag_ctx.emit_const_cycle(const_def.name, const_def.loc());
            }
            None => {}
        };

        self.evaluated_consts.insert_no_prev(const_id, State::InProgress);

        let mut scope = Scope {
            eval: self,
            diag_ctx,
            source: const_def.source_id,
            func: Some(Function { ret_type: TypeId::NEVER, ret_type_span: None }),
            comptime: true,
            bindings: DenseIndexMap::new(),
            mir_types: IndexVec::new(),
        };

        for &instr in &scope.hir.block_instrs[const_def.body] {
            scope.eval_instr(instr).expect("todo: handle comptime diverge");
        }

        let value = scope.bindings[const_def.result].state.map(|state| match state {
            LocalState::Comptime(vid) => vid,
            LocalState::Runtime(_) => {
                unreachable!("local in comptime set to runtime instead of poisoned")
            }
        });
        self.evaluated_consts.insert(const_id, State::Done(value));
        self.try_name_type(const_def.name, value);

        value
    }

    fn try_name_type(&mut self, name: StrId, value: MaybePoisoned<ValueId>) {
        if let Ok(Value::Type(ty)) = value.map(|vid| self.values.lookup(vid)) {
            self.types.try_set_struct_name(ty, name);
        }
    }

    pub fn lower_entrypoint(
        &mut self,
        block: hir::BlockId,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> mir::FnId {
        let source = self.hir.entry_source;
        let mut scope = Scope {
            eval: self,
            diag_ctx,
            source,
            func: None,
            comptime: false,
            bindings: DenseIndexMap::new(),
            mir_types: IndexVec::new(),
        };

        let body = scope.eval_entry_point_body(block);

        let fn_id1 = scope.eval.mir_fn_locals.push_copy_slice(&scope.mir_types);
        let fn_id2 =
            self.mir_fns.push(mir::FnDef { body, param_count: 0, return_type: TypeId::NEVER });
        assert_eq!(fn_id1, fn_id2);

        fn_id1
    }
}
