use hashbrown::{HashMap, HashSet};
use sir_data::StaticAllocId;
use std::num::NonZero;

mod bump_allocate_all;

pub use bump_allocate_all::BumpAllocateAll;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EvmMemAddr(NonZero<u32>);

impl EvmMemAddr {
    #[track_caller]
    pub const fn new(offset: u32) -> EvmMemAddr {
        EvmMemAddr(NonZero::new(!offset).expect("NonZero<u32> overflow"))
    }

    pub const fn get(self) -> u32 {
        !self.0.get()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DynFreePointer {
    pub store_slot: EvmMemAddr,
    pub start_value: EvmMemAddr,
}

#[derive(Debug, Clone)]
pub struct Layout {
    pub dyn_free_pointer: Option<DynFreePointer>,
    pub switch_store: Option<EvmMemAddr>,
    pub alloc_start: HashMap<StaticAllocId, EvmMemAddr>,
    pub alloc_needs_zeroing: HashSet<StaticAllocId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evm_addr_rountrip() {
        assert_eq!(EvmMemAddr::new(0).get(), 0);
        assert_eq!(EvmMemAddr::new(1).get(), 1);
        assert_eq!(EvmMemAddr::new(0xfffffffe).get(), 0xfffffffe);
    }
}
