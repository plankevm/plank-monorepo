use plank_core::{DenseIndexMap, IndexVec, list_of_lists::ListOfLists};
use plank_hir::{self as hir, ConstId, Hir};
use plank_mir as mir;
use plank_session::{MaybePoisoned, Session};
use plank_values::{TypeId, TypeInterner, ValueId, ValueInterner};

use crate::{
    diagnostics::DiagCtx,
    scope::{Function, LocalState, Scope},
};

enum ConstState {
    InProgress,
    Evaluated(MaybePoisoned<ValueId>),
}

pub(crate) struct Evaluator<'a> {
    // MIR
    pub mir_blocks: ListOfLists<mir::BlockId, mir::Instruction>,
    pub mir_args: ListOfLists<mir::ArgsId, mir::LocalId>,
    pub mir_fns: IndexVec<mir::FnId, mir::FnDef>,
    pub mir_fn_locals: ListOfLists<mir::FnId, TypeId>,
    pub types: TypeInterner,

    pub evaluated_consts: DenseIndexMap<ConstId, ConstState>,
    pub values: &'a mut ValueInterner,
    pub hir: &'a Hir,

    pub diag_ctx: DiagCtx<'a>,

    pub instr_stack_buf: Vec<mir::Instruction>,
    pub types_buf: Vec<TypeId>,
    pub locals_buf: Vec<mir::LocalId>,
    pub values_buf: Vec<ValueId>,
}

impl<'a> Evaluator<'a> {
    pub fn new(hir: &'a Hir, values: &'a mut ValueInterner, session: &'a mut Session) -> Self {
        Evaluator {
            mir_blocks: ListOfLists::new(),
            mir_fns: IndexVec::new(),
            mir_fn_locals: ListOfLists::new(),
            mir_args: ListOfLists::new(),
            types: TypeInterner::new(),

            evaluated_consts: DenseIndexMap::new(),
            values,
            hir,

            diag_ctx: DiagCtx::new(session),

            instr_stack_buf: Vec::new(),
            types_buf: Vec::new(),
            locals_buf: Vec::new(),
            values_buf: Vec::new(),
        }
    }

    pub fn evaluate_const(&mut self, const_id: ConstId) -> MaybePoisoned<ValueId> {
        match self.evaluated_consts.get(const_id) {
            Some(&ConstState::Evaluated(vid)) => return vid,
            Some(ConstState::InProgress) => todo!("cyclical const error"),
            None => {}
        };

        self.evaluated_consts.insert_no_prev(const_id, ConstState::InProgress);

        let const_def = self.hir.consts[const_id];
        let mut scope = Scope {
            eval: self,
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
        self.evaluated_consts.insert(const_id, ConstState::Evaluated(value));

        value
    }

    pub fn lower_entrypoint(&mut self, block: hir::BlockId) -> mir::FnId {
        let source = self.hir.entry_source;
        let mut scope = Scope {
            eval: self,
            source,
            func: Some(Function { ret_type: TypeId::NEVER, ret_type_span: None }),
            comptime: false,
            bindings: DenseIndexMap::new(),
            mir_types: IndexVec::new(),
        };

        let body = scope.eval_fn_body(block);

        let fn_id1 = scope.eval.mir_fn_locals.push_copy_slice(&scope.mir_types);
        let fn_id2 =
            self.mir_fns.push(mir::FnDef { body, param_count: 0, return_type: TypeId::NEVER });
        assert_eq!(fn_id1, fn_id2);

        fn_id1
    }
}
