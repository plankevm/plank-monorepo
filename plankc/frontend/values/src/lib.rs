mod bignum_interner;
mod type_interner;

use plank_core::newtype_index;

newtype_index! {
    pub struct ValueId;
}

impl ValueId {
    pub const VOID: Self = ValueId::new(0);
    pub const FALSE: Self = ValueId::new(1);
    pub const TRUE: Self = ValueId::new(2);
}

pub use bignum_interner::{BigNumId, BigNumInterner};
pub use plank_session::TypeId;
pub use type_interner::{StructInfo, Type, TypeInterner};
