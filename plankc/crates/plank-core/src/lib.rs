pub mod bigint;
pub mod chunked_arena;
pub mod const_print;
pub mod dense_index_map;
pub mod dense_index_set;
pub mod index;
pub mod index_vec;
pub mod intern;
pub mod list_of_lists;
pub mod must_use;
pub mod span;
pub mod vec_buf;

pub use crate::{
    dense_index_map::DenseIndexMap,
    dense_index_set::DenseIndexSet,
    index::Idx,
    index_vec::{IndexVec, RelSlice, RelSliceMut},
    span::{IncIterable, Span},
};

/// Alias denoting an arena allocated `T`.
pub type ABox<'arena, T> = &'arena mut T;

/// Core crate assumption.
const _USIZE_AT_LEAST_U32: () = const {
    assert!(u32::BITS <= usize::BITS);
};

#[derive(Debug)]
pub struct LoopLimit {
    count: u32,
    max: u32,
}

impl LoopLimit {
    /// Initialize a loop limiter with a max of 100M
    pub fn new() -> Self {
        Self { count: 0, max: 100_000_000 }
    }

    pub fn max(max: u32) -> Self {
        Self { count: 0, max }
    }

    #[track_caller]
    pub fn tick(&mut self) {
        self.count += 1;
        assert!(self.count <= self.max, "loop limit hit ({})", self.max);
    }
}

impl Default for LoopLimit {
    fn default() -> Self {
        Self::new()
    }
}
