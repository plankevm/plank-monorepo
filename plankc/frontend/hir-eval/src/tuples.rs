use crate::scope::{EvalValue, Scope};
use plank_hir as hir;
use plank_session::{MaybePoisoned, SourceSpan};
use plank_values::TypeId;

impl<'eval, 'ctx> Scope<'eval, 'ctx> {
    pub(crate) fn eval_tuple_type(
        &mut self,
        elements: hir::ElementsId,
    ) -> MaybePoisoned<TypeId> {
        todo!()
    }

    pub(crate) fn eval_tuple_lit(
        &mut self,
        elements: hir::ElementsId,
        lit_span: SourceSpan,
    ) -> MaybePoisoned<EvalValue> {
        todo!()
    }
}
