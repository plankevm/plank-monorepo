use alloy_primitives as _;
use hashbrown as _;
use plank_evm as _;

use plank_hir::Hir;
use plank_mir::Mir;
use plank_session::Session;
use plank_values::ValueInterner;

mod builtins;
mod diagnostics;
mod evaluator;
mod scope;

pub(crate) use evaluator::Evaluator;

#[cfg(test)]
mod tests;

pub fn evaluate(hir: &Hir, values: &mut ValueInterner, session: &mut Session) -> Mir {
    let mut evaluator = Evaluator::new(hir, values, session);

    let init = evaluator.lower_entrypoint(hir.init);
    let run = hir.run.map(|run| evaluator.lower_entrypoint(run));

    for const_id in hir.consts.iter_idx() {
        let _ = evaluator.evaluate_const(const_id);
    }

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
