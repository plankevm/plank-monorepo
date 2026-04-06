use plank_core::{IndexVec, list_of_lists::ListOfLists};
use plank_hir::{self as hir, Hir};
use plank_mir as mir;
use plank_values::{TypeId, TypeInterner, ValueInterner};

use crate::diagnostics::DiagCtx;

pub(crate) struct Evaluator<'a> {
    // MIR
    pub mir_blocks: ListOfLists<mir::BlockId, mir::Instruction>,
    pub mir_args: ListOfLists<mir::ArgsId, mir::LocalId>,
    pub mir_fns: IndexVec<mir::FnId, mir::FnDef>,
    pub mir_fn_locals: ListOfLists<mir::FnId, TypeId>,
    pub types: TypeInterner,

    pub values: &'a mut ValueInterner,
    pub hir: &'a Hir,

    pub diag_ctx: DiagCtx<'a>,
}

impl Evaluator<'_> {
    pub fn lower_entrypoint(&mut self, block: hir::BlockId) -> mir::FnId {
        todo!()
    }
}
