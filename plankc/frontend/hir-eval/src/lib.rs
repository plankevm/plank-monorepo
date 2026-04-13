use alloy_primitives as _;
use hashbrown as _;
use plank_evm as _;

use plank_hir::Hir;
use plank_mir::Mir;
use plank_session::Session;
use plank_values::ValueInterner;

mod buffers;
mod builtins;
mod diagnostics;
mod evaluator;
mod functions;
mod scope;
mod structs;

pub(crate) use evaluator::Evaluator;

#[cfg(test)]
mod tests;

pub fn evaluate(hir: &Hir, values: &mut ValueInterner, session: &mut Session) -> Mir {
    let mut evaluator = Evaluator::new(hir, values);
    let mut diag_ctx = diagnostics::DiagCtx::new(session);

    let init = evaluator.lower_entrypoint(hir.init, &mut diag_ctx);
    let run = hir.run.map(|run| evaluator.lower_entrypoint(run, &mut diag_ctx));

    for const_id in hir.consts.iter_idx() {
        let _ = evaluator.evaluate_const(const_id, &mut diag_ctx);
    }

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
