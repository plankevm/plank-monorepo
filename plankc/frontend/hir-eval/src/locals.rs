use plank_core::{DenseIndexMap, IndexVec};
use plank_hir::{self as hir, ExprKind, FieldsId, InstructionKind, StructDef, StructDefId};
use plank_mir as mir;
use plank_session::{
    EvmBuiltin, MaybePoisoned, Poisoned, SourceId, SourceSpan, SrcLoc, poison::MaybePoisonedResult,
};
use plank_values::{StructInfo, Type, TypeId, Value, ValueId};

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

#[derive(Debug)]
pub(crate) struct ScopeLocals {
    pub bindings: DenseIndexMap<hir::LocalId, Local>,
    pub mir_types: IndexVec<mir::LocalId, TypeId>,
}

impl ScopeLocals {
    pub fn new() -> Self {
        Self { bindings: DenseIndexMap::new(), mir_types: IndexVec::new() }
    }

    pub fn mir_types(&self, local: mir::LocalId) -> TypeId {
        self.mir_types[local]
    }

    pub fn bindings(&self, local: hir::LocalId) -> Local {
        self.bindings[local]
    }
}
