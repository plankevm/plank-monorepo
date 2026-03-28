use plank_core::{DenseIndexMap, IndexVec};
use plank_hir::{self as hir};
use plank_mir::{self as mir};
use plank_session::SrcLoc;
use plank_values::{TypeId, ValueId};

use crate::value::ValueInterner;

pub(crate) struct TypeMismatchError {
    pub expected_ty: TypeId,
    pub received_ty: TypeId,
}

pub(crate) struct TypeUnificationError {
    pub existing_def: SrcLoc,
    pub existing_ty: TypeId,
    pub new_ty: TypeId,
}

#[derive(Debug, Default)]
pub(crate) struct Locals {
    def_loc: DenseIndexMap<hir::LocalId, SrcLoc>,
    hir_to_mir: DenseIndexMap<hir::LocalId, mir::LocalId>,
    value: DenseIndexMap<hir::LocalId, ValueId>,
    types: IndexVec<mir::LocalId, TypeId>,
}

impl Locals {
    pub fn alloc_anonymous_mir(&mut self, ty: TypeId) -> mir::LocalId {
        self.types.push(ty)
    }

    pub fn comptime(&self, hir: hir::LocalId) -> Option<ValueId> {
        self.value.get(hir).copied()
    }

    pub fn get_mir(&self, hir: hir::LocalId) -> Option<mir::LocalId> {
        self.hir_to_mir.get(hir).copied()
    }

    pub fn def_loc(&self, hir: hir::LocalId) -> SrcLoc {
        self.def_loc[hir]
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

    fn checked_alloc_mir(&mut self, hir: hir::LocalId, ty: TypeId, loc: SrcLoc) -> mir::LocalId {
        let mir = self.types.push(ty);
        assert!(self.hir_to_mir.insert(hir, mir).is_none());
        assert!(self.def_loc.insert(hir, loc).is_none());
        mir
    }

    pub fn define_unset(&mut self, hir: hir::LocalId, ty: TypeId, loc: SrcLoc) {
        self.checked_alloc_mir(hir, ty, loc);
    }

    pub fn set_comptime_only(&mut self, hir: hir::LocalId, value: ValueId, loc: SrcLoc) {
        assert!(!self.hir_to_mir.contains(hir));
        let prev = self.value.insert(hir, value);
        assert!(prev.is_none(), "comptime-only local already set");
        assert!(self.def_loc.insert(hir, loc).is_none());
    }

    pub fn set(
        &mut self,
        hir: hir::LocalId,
        ty: TypeId,
        loc: SrcLoc,
        comptime_known: Option<ValueId>,
    ) -> Result<mir::LocalId, TypeMismatchError> {
        if let Some(&mir) = self.hir_to_mir.get(hir) {
            let expected_ty = self.types[mir];
            if !ty.is_assignable_to(expected_ty) {
                return Err(TypeMismatchError { expected_ty, received_ty: ty });
            }
            if let Some(value) = comptime_known {
                let prev = self.value.insert(hir, value);
                assert!(prev.is_none());
            }
            return Ok(mir);
        }
        Ok(self.checked_alloc_mir(hir, ty, loc))
    }

    pub fn branch_set(
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
            assert!(self.value.get(hir).is_none());
            return Ok(mir);
        }
        Ok(self.checked_alloc_mir(hir, ty, loc))
    }

    pub fn assign(
        &mut self,
        hir: hir::LocalId,
        new_ty: TypeId,
    ) -> Result<mir::LocalId, TypeMismatchError> {
        let mir = self.hir_to_mir[hir];
        self.value.remove(hir);
        let expected_ty = self.mir_type(mir);
        if !new_ty.is_assignable_to(expected_ty) {
            return Err(TypeMismatchError { expected_ty, received_ty: new_ty });
        }
        Ok(mir)
    }

    pub fn get_type(&self, hir: hir::LocalId, values: &ValueInterner) -> TypeId {
        if let Some(mir) = self.get_mir(hir) {
            return self.mir_type(mir);
        }
        values.type_of_value(self.value[hir])
    }
}
