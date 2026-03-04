use crate::Idx;
use std::marker::PhantomData;

/// A dense map from typed indices to values, backed by a `Vec<Option<V>>`.
///
/// This is a more efficient alternative to `HashMap<I, V>` when indices are densely packed
/// typed `Idx` values. It provides O(1) lookup and insertion, auto-growing to accommodate
/// any index on insert.
#[derive(Clone)]
pub struct DenseIndexMap<I: Idx, V> {
    inner: Vec<Option<V>>,
    _marker: PhantomData<I>,
}

impl<I: Idx, V> DenseIndexMap<I, V> {
    /// Creates a new empty `DenseIndexMap`.
    #[inline]
    pub fn new() -> Self {
        Self { inner: Vec::new(), _marker: PhantomData }
    }

    /// Creates a new `DenseIndexMap` with capacity for at least `capacity` index slots.
    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Self { inner: Vec::with_capacity(capacity), _marker: PhantomData }
    }

    /// Returns a reference to the value associated with `key`, or `None` if absent.
    #[inline]
    pub fn get(&self, key: I) -> Option<&V> {
        self.inner.get(key.idx())?.as_ref()
    }

    /// Inserts a value at `key`, returning the previous value if one was present.
    ///
    /// Auto-grows the backing storage to fit `key` if necessary.
    pub fn insert(&mut self, key: I, value: V) -> Option<V> {
        let idx = key.idx();
        if idx >= self.inner.len() {
            let new_len = idx.checked_add(1).expect("index overflow");
            self.inner.resize_with(new_len, || None);
        }
        // SAFETY: `resize_with` above ensures `idx < self.inner.len()`.
        unsafe { self.inner.get_unchecked_mut(idx) }.replace(value)
    }
}

impl<I: Idx, V> Default for DenseIndexMap<I, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Idx, V: std::fmt::Debug> std::fmt::Debug for DenseIndexMap<I, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for (i, slot) in self.inner.iter().enumerate() {
            if let Some(v) = slot {
                let key = I::ZERO + i as u32;
                map.entry(&key, v);
            }
        }
        map.finish()
    }
}

impl<I: Idx, V> std::ops::Index<I> for DenseIndexMap<I, V> {
    type Output = V;

    fn index(&self, index: I) -> &Self::Output {
        self.get(index).expect("index out of bounds")
    }
}

#[cfg(test)]
mod tests {
    use crate::newtype_index;

    use super::*;

    newtype_index! {
        struct TestIdx;
    }

    #[test]
    fn test_new_empty() {
        let map: DenseIndexMap<TestIdx, i32> = DenseIndexMap::new();
        assert_eq!(map.get(TestIdx::new(0)), None);
        assert_eq!(map.get(TestIdx::new(100)), None);
    }

    #[test]
    fn test_insert_and_get() {
        let mut map: DenseIndexMap<TestIdx, i32> = DenseIndexMap::new();

        assert_eq!(map.insert(TestIdx::new(5), 42), None);
        assert_eq!(map.get(TestIdx::new(5)), Some(&42));
        assert_eq!(map.get(TestIdx::new(4)), None);
        assert_eq!(map.get(TestIdx::new(6)), None);
    }

    #[test]
    fn test_insert_overwrite() {
        let mut map: DenseIndexMap<TestIdx, &str> = DenseIndexMap::new();

        assert_eq!(map.insert(TestIdx::new(3), "first"), None);
        assert_eq!(map.get(TestIdx::new(3)), Some(&"first"));

        assert_eq!(map.insert(TestIdx::new(3), "second"), Some("first"));
        assert_eq!(map.get(TestIdx::new(3)), Some(&"second"));
    }

    #[test]
    fn test_sparse_insert() {
        let mut map: DenseIndexMap<TestIdx, i32> = DenseIndexMap::new();

        map.insert(TestIdx::new(1000), 99);
        assert_eq!(map.get(TestIdx::new(1000)), Some(&99));
        assert_eq!(map.get(TestIdx::new(0)), None);
        assert_eq!(map.get(TestIdx::new(999)), None);
    }

    #[test]
    fn test_with_capacity() {
        let map: DenseIndexMap<TestIdx, i32> = DenseIndexMap::with_capacity(256);
        assert_eq!(map.get(TestIdx::new(0)), None);
        assert_eq!(map.get(TestIdx::new(255)), None);
    }

    #[test]
    fn test_default() {
        let map: DenseIndexMap<TestIdx, i32> = Default::default();
        assert_eq!(map.get(TestIdx::new(0)), None);
    }

    #[test]
    fn test_multiple_entries() {
        let mut map: DenseIndexMap<TestIdx, i32> = DenseIndexMap::new();

        map.insert(TestIdx::new(0), 10);
        map.insert(TestIdx::new(2), 30);
        map.insert(TestIdx::new(4), 50);

        assert_eq!(map.get(TestIdx::new(0)), Some(&10));
        assert_eq!(map.get(TestIdx::new(1)), None);
        assert_eq!(map.get(TestIdx::new(2)), Some(&30));
        assert_eq!(map.get(TestIdx::new(3)), None);
        assert_eq!(map.get(TestIdx::new(4)), Some(&50));
    }
}
