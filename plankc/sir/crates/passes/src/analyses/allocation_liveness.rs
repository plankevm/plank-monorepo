use sir_data::{BasicBlockId, IndexVec, LocalId, OperationIdx, StaticAllocId, newtype_index};

newtype_index! {
    pub struct AllocId;
}

#[derive(Debug, Clone, Copy)]
pub enum AllocKind {
    Static { size: u32, id: StaticAllocId },
    Dynamic { size_local: LocalId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalStart {
    LiveIn,
    At(OperationIdx),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalEnd {
    LiveOut,
    At(OperationIdx),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: IntervalStart,
    pub end: IntervalEnd,
}

#[derive(Debug, Clone)]
pub struct AllocData {
    pub def_block: BasicBlockId,
    pub def_op: OperationIdx,
    pub kind: AllocKind,
    pub escapes: bool,
    pub intervals: Vec<(BasicBlockId, Interval)>,
}

#[derive(Debug, Clone)]
pub struct AllocationLiveness {
    pub allocations: IndexVec<AllocId, AllocData>,
    pub local_to_alloc: IndexVec<LocalId, Option<AllocId>>,
}
