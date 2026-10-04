use std::marker::PhantomData;

use crate::{Idx, Span};

/// A borrowed slice that is indexed by `I` from zero rather than `usize`.
#[repr(transparent)]
pub struct IndexSlice<I: Idx, T> {
    _idx: PhantomData<I>,
    inner: [T],
}

impl<I: Idx, T> IndexSlice<I, T> {
    #[inline]
    pub fn from_raw(raw: &[T]) -> &Self {
        // Safety: `IndexSlice` is `repr(transparent)` over `[T]`, `PhantomData` is a ZST.
        unsafe { &*(raw as *const [T] as *const Self) }
    }

    #[inline]
    pub fn from_raw_mut(raw: &mut [T]) -> &mut Self {
        // Safety: `IndexSlice` is `repr(transparent)` over `[T]`, `PhantomData` is a ZST.
        unsafe { &mut *(raw as *mut [T] as *mut Self) }
    }

    #[inline]
    pub fn empty<'a>() -> &'a Self {
        Self::from_raw(&[])
    }

    #[inline]
    pub fn as_raw_slice(&self) -> &[T] {
        &self.inner
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn len_idx(&self) -> I {
        I::try_from(self.len()).unwrap_or_else(|_| panic!("holds more than I::MAX elements"))
    }

    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.inner.iter()
    }

    pub fn iter_idx(&self) -> impl Iterator<Item = I> + use<I, T> {
        Span::new(I::ZERO, self.len_idx()).iter()
    }

    pub fn enumerate_idx(&self) -> impl Iterator<Item = (I, &T)> {
        self.iter_idx().zip(self.iter())
    }

    #[inline]
    pub fn get(&self, index: I) -> Option<&T> {
        self.inner.get(index.idx())
    }
}

impl<I: Idx, T> std::ops::Index<I> for IndexSlice<I, T> {
    type Output = T;

    fn index(&self, index: I) -> &Self::Output {
        &self.inner[index.idx()]
    }
}

impl<I: Idx, T> std::ops::IndexMut<I> for IndexSlice<I, T> {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.inner[index.idx()]
    }
}

impl<I: Idx, T> std::ops::Index<std::ops::RangeTo<I>> for IndexSlice<I, T> {
    type Output = IndexSlice<I, T>;

    fn index(&self, range: std::ops::RangeTo<I>) -> &Self::Output {
        Self::from_raw(&self.inner[..range.end.idx()])
    }
}

impl<'a, I: Idx, T> IntoIterator for &'a IndexSlice<I, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<I: Idx, T: PartialEq> PartialEq for IndexSlice<I, T> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<I: Idx, T: Eq> Eq for IndexSlice<I, T> {}

impl<I: Idx, T: std::hash::Hash> std::hash::Hash for IndexSlice<I, T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.hash(state)
    }
}

impl<I: Idx, T: std::fmt::Debug> std::fmt::Debug for IndexSlice<I, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}
