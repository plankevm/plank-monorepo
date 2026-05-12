use hashbrown::{HashMap, HashSet};
use sir_data::StaticAllocId;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EvmMemAddr(u32);

#[derive(Debug, Clone)]
pub struct Layout {
    pub dyn_free_pointer_slot: Option<EvmMemAddr>,
    pub switch_store: Option<EvmMemAddr>,
    pub alloc_start: HashMap<StaticAllocId, EvmMemAddr>,
    pub alloc_needs_zeroing: HashSet<StaticAllocId>,
}

pub trait LayoutGenerator {
    fn generate(&mut self) -> Layout;
}
