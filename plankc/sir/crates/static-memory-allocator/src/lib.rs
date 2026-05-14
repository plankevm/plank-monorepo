use hashbrown::{HashMap, HashSet};
use plank_core::list_of_lists::ListOfLists;
use sir_data::{BasicBlockId, EthIRProgram, StaticAllocId};
use sir_passes::AnalysesStore;
use sir_stack_scheduling::stack::StackOps;
use std::num::NonZero;

mod noob;

pub use noob::EvmStaticAllocator;

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

pub enum EntrypointKind {
    Init,
    Run,
}

pub trait LayoutGenerator {
    fn generate(
        &mut self,
        program: &EthIRProgram,
        analyses: &AnalysesStore,
        entrypoint: EntrypointKind,
        stack_ops: &ListOfLists<BasicBlockId, StackOps>,
    ) -> Layout;
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
