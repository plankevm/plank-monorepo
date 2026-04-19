mod bignum_interner;
pub mod builtins;
mod type_interner;
mod value_interner;

pub use alloy_primitives::{U256, uint};
use plank_core::{const_print::const_assert_mem_size, newtype_index};
use plank_session::SourceSpan;
pub use type_interner::*;
pub use value_interner::*;

newtype_index! {
    pub struct ValueId;
    pub struct FnDefId;
    pub struct ConstId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DefOrigin {
    Local(SourceSpan),
    Const(ConstId),
}

const _DEF_ORIGIN_SIZE: () = const_assert_mem_size::<DefOrigin>(8);

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
