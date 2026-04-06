use plank_hir::Hir;
use plank_mir::{self as mir, Mir};
use plank_session::SrcLoc;
use plank_values::TypeId;

use crate::Evaluator;

pub(crate) enum Local {
    Runtime(mir::LocalId),
    Comptime(ValueId),
}

pub(crate) struct FnScope<'a> {
    pub eval: &'a mut Evaluator<'a>,

    pub expected_ret_type: TypeId,
    pub expected_ret_loc: Option<SrcLoc>,
}
