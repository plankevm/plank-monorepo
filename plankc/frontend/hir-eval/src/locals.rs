use plank_mir as mir;
use plank_session::{MaybePoisoned, SourceSpan};
use plank_values::ValueId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Local {
    pub state: MaybePoisoned<LocalState>,
    pub span: SourceSpan,
}

impl Local {
    pub fn poisoned(self) -> MaybePoisoned<(LocalState, SourceSpan)> {
        let state = self.state?;
        Ok((state, self.span))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalState {
    Runtime(mir::LocalId),
    Comptime(ValueId),
}
