use alloy_primitives as _;
use plank_evm as _;

use hashbrown::HashMap;
use plank_core::{IndexVec, index_vec, list_of_lists::ListOfLists};
use plank_hir::{ConstId, Hir};
use plank_mir::{self as mir, Mir};
use plank_session::{Session, StrId};
use plank_values::{TypeId, TypeInterner, ValueId, ValueInterner};

#[cfg(test)]
mod tests;

pub fn evaluate(hir: &Hir, values: &mut ValueInterner, session: &mut Session) -> Mir {
    todo!()
}
