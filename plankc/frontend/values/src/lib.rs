mod bignum_interner;
mod type_interner;
mod value_interner;

use plank_core::newtype_index;

newtype_index! {
    pub struct ValueId;
}

impl ValueId {
    pub const VOID: Self = ValueId::new(0);
    pub const FALSE: Self = ValueId::new(1);
    pub const TRUE: Self = ValueId::new(2);
}

impl From<bool> for ValueId {
    fn from(value: bool) -> Self {
        match value {
            false => Self::FALSE,
            true => Self::TRUE,
        }
    }
}

pub use alloy_primitives::{U256, uint};
pub use plank_session::TypeId;
pub use type_interner::{StructInfo, Type, TypeInterner};
pub use value_interner::*;
