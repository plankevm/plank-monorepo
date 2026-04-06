use plank_session::SrcLoc;
use plank_values::TypeId;

use crate::Evaluator;

pub(crate) struct FnScope<'a> {
    pub eval: &'a mut Evaluator<'a>,

    pub expected_ret_type: TypeId,
    pub expected_ret_loc: SrcLoc,
}
