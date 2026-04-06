use alloy_primitives as _;
use hashbrown as _;
use plank_evm as _;

use plank_core::{IndexVec, list_of_lists::ListOfLists};
use plank_hir::Hir;
use plank_mir::{self as mir, Mir};
use plank_session::{Session, StrId};
use plank_values::{TypeId, TypeInterner, ValueId, ValueInterner};

mod diagnostics;
mod evaluator;
mod fn_scope;

pub(crate) use evaluator::Evaluator;

use crate::diagnostics::DiagCtx;

#[cfg(test)]
mod tests;

pub fn evaluate(hir: &Hir, values: &mut ValueInterner, session: &mut Session) -> Mir {
    let mut evaluator = Evaluator {
        mir_blocks: ListOfLists::new(),
        mir_fns: IndexVec::new(),
        mir_fn_locals: ListOfLists::new(),
        mir_args: ListOfLists::new(),
        types: TypeInterner::new(),

        values,
        hir,

        diag_ctx: DiagCtx::new(session),
    };

    let init = evaluator.lower_entrypoint(hir.init);
    let run = hir.run.map(|run| evaluator.lower_entrypoint(run));

    // 1. Eval init as fn
    // 2. Eval run (if present) as fn
    // 3. Ensure remaining constants are evaluated for diagnostics

    Mir {
        blocks: evaluator.mir_blocks,
        args: evaluator.mir_args,
        fns: evaluator.mir_fns,
        fn_locals: evaluator.mir_fn_locals,
        types: evaluator.types,
        init,
        run,
    }
}
