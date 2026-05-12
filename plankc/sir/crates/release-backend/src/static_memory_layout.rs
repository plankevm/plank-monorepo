use hashbrown::{HashMap, HashSet};
use sir_data::StaticAllocId;

#[repr(transparent)]
pub(crate) struct EvmMemAddr(u32);

pub(crate) struct StaticMemoryLayout {
    pub switch_store: Option<EvmMemAddr>,
    pub alloc_start: HashMap<StaticAllocId, EvmMemAddr>,
    pub alloc_needs_zeroing: HashSet<StaticAllocId>,
}
