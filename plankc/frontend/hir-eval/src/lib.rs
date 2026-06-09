use alloy_primitives as _;
use hashbrown as _;
use plank_evm as _;

use plank_hir::Hir;
use plank_mir::Mir;
use plank_session::{Session, SourceId};
use plank_values::{TypeInterner, ValueInterner};

mod buffers;
mod builtins;
mod diagnostics;
mod evaluator;
mod functions;
mod operators;
mod quota;
mod scope;
mod structs;
mod tuples;

pub(crate) use evaluator::Evaluator;

use crate::{functions::EvaluatedFunctionCache, operators::OperatorTable};

#[cfg(test)]
mod tests;

pub fn evaluate(
    hir: &Hir,
    core_ops_source: Option<SourceId>,
    values: &mut ValueInterner,
    session: &mut Session,
) -> Mir {
    let types = TypeInterner::new();
    let evaluated_fns_cache = EvaluatedFunctionCache::new();
    let mut evaluator = Evaluator::new(hir, &types, &evaluated_fns_cache, values);
    let mut diag_ctx = diagnostics::DiagCtx::new(session, &types);

    evaluator.operator_table = match core_ops_source {
        Some(core_ops_source) => {
            OperatorTable::with_std_ops(hir, core_ops_source, &mut evaluator, &mut diag_ctx)
        }
        None => OperatorTable::new(),
    };

    let mut init = None;
    let mut run = None;
    for (entry_id, &entry_point) in hir.entry_points.enumerate_idx() {
        let fn_id = evaluator.lower_entrypoint(entry_point, &mut diag_ctx);
        if entry_id == hir.init {
            init = Some(fn_id);
        }
        if Some(entry_id) == hir.run {
            run = Some(fn_id);
        }
    }
    let init = init.expect("HIR init entry point must exist in entry_points");

    for const_id in hir.consts.iter_idx() {
        let _ = evaluator.evaluate_const(const_id, &mut diag_ctx);
    }

    Mir {
        blocks: evaluator.mir_blocks,
        args: evaluator.mir_args,
        fns: evaluator.mir_fns,
        fn_locals: evaluator.mir_fn_locals,
        types,
        init,
        run,
    }
}
