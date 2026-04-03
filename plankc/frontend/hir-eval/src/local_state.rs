use plank_core::{DenseIndexMap, IndexVec};
use plank_hir::{self as hir};
use plank_mir::{self as mir};
use plank_session::SrcLoc;
use plank_values::TypeId;

pub(crate) struct TypeMismatchError {
    pub expected_ty: TypeId,
    pub received_ty: TypeId,
}

pub(crate) struct TypeUnificationError {
    pub existing_def: SrcLoc,
    pub existing_ty: TypeId,
    pub new_ty: TypeId,
}

/// Tracks runtime-known HIR locals and their mapping to MIR locals. MIR locals must each have a
/// known type.
#[derive(Debug, Default)]
pub(crate) struct RuntimeLocals {
    def_loc: DenseIndexMap<hir::LocalId, SrcLoc>,
    hir_to_mir: DenseIndexMap<hir::LocalId, mir::LocalId>,
    types: IndexVec<mir::LocalId, TypeId>,
}

impl RuntimeLocals {
    /// Creates an independent MIR local that isn't tied to any HIR local.
    pub fn alloc_anonymous_mir(&mut self, ty: TypeId) -> mir::LocalId {
        self.types.push(ty)
    }

    pub fn get_mir(&self, hir: hir::LocalId) -> Option<mir::LocalId> {
        self.hir_to_mir.get(hir).copied()
    }

    pub fn def_loc(&self, hir: hir::LocalId) -> SrcLoc {
        self.def_loc[hir]
    }

    pub fn register_def_loc(&mut self, hir: hir::LocalId, loc: SrcLoc) {
        assert!(self.def_loc.insert(hir, loc).is_none());
    }

    pub fn hir_to_mir(&self, hir: hir::LocalId) -> mir::LocalId {
        self.hir_to_mir[hir]
    }

    pub fn mir_type(&self, mir: mir::LocalId) -> TypeId {
        self.types[mir]
    }

    pub fn mir_types(&self) -> impl Iterator<Item = TypeId> {
        self.types.iter().copied()
    }

    pub fn associate_hir_to_new_mir(
        &mut self,
        hir: hir::LocalId,
        ty: TypeId,
        loc: SrcLoc,
    ) -> mir::LocalId {
        let mir = self.types.push(ty);
        assert!(self.hir_to_mir.insert(hir, mir).is_none());
        assert!(self.def_loc.insert(hir, loc).is_none());
        mir
    }

    pub fn set(
        &mut self,
        hir: hir::LocalId,
        ty: TypeId,
        loc: SrcLoc,
    ) -> Result<mir::LocalId, TypeMismatchError> {
        let mir = if let Some(&mir) = self.hir_to_mir.get(hir) {
            let expected_ty = self.types[mir];
            if !ty.is_assignable_to(expected_ty) {
                return Err(TypeMismatchError { expected_ty, received_ty: ty });
            }
            mir
        } else {
            self.associate_hir_to_new_mir(hir, ty, loc)
        };
        Ok(mir)
    }

    pub fn set_from_branch(
        &mut self,
        hir: hir::LocalId,
        ty: TypeId,
        loc: SrcLoc,
    ) -> Result<mir::LocalId, TypeUnificationError> {
        if let Some(&mir) = self.hir_to_mir.get(hir) {
            let expected_ty = &mut self.types[mir];
            if !expected_ty.unify(ty) {
                return Err(TypeUnificationError {
                    existing_def: self.def_loc[hir],
                    existing_ty: *expected_ty,
                    new_ty: ty,
                });
            };
            return Ok(mir);
        }
        Ok(self.associate_hir_to_new_mir(hir, ty, loc))
    }

    pub fn handle_assign(
        &mut self,
        hir: hir::LocalId,
        new_ty: TypeId,
    ) -> Result<mir::LocalId, TypeMismatchError> {
        let mir = self.hir_to_mir[hir];
        let expected_ty = self.mir_type(mir);
        if !new_ty.is_assignable_to(expected_ty) {
            return Err(TypeMismatchError { expected_ty, received_ty: new_ty });
        }
        Ok(mir)
    }
}
