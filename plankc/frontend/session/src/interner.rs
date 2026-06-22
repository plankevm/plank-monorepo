use plank_core::{intern::BytesInterner, newtype_index};

newtype_index! {
    /// Index into the combined backing store. Kept private so that `StrId` /
    /// `BytesId` can only be minted inside this module.
    struct InternIdx;
}

/// Index of an interned string.
///
/// Sealed: outside this module it can only be obtained from
/// [`Interner::intern_str`] (or the built-in known name consts, which are validated
/// against the interner on session construction). This guarantees the indexed
/// content is valid UTF-8 as long as the id is resolved against the interner
/// that minted it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StrId(InternIdx);

/// Index of an interned byte string (arbitrary, not necessarily UTF-8).
///
/// Sealed: outside this module it can only be obtained from
/// [`Interner::intern_bytes`] or by converting a [`StrId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BytesId(InternIdx);

/// Id of the empty byte string, validated on [`Interner`] construction.
pub const EMPTY_BYTES: BytesId = BytesId(InternIdx::new(0));

impl StrId {
    /// Mints a `StrId` without interning anything.
    ///
    /// Only sound for the built-in known name table: `inject_builtins` asserts that
    /// every const produced this way matches the id the session's interner
    /// actually assigns to the corresponding name.
    pub(crate) const fn from_builtin_index(raw: u32) -> Self {
        Self(InternIdx::new(raw))
    }
}

impl From<StrId> for BytesId {
    fn from(id: StrId) -> Self {
        Self(id.0)
    }
}

pub struct Interner {
    bytes: BytesInterner<InternIdx>,
}

impl Interner {
    pub fn new() -> Self {
        let mut this = Self { bytes: BytesInterner::new() };
        assert_eq!(this.intern_bytes(b""), EMPTY_BYTES);
        this
    }

    pub fn intern_str(&mut self, string: &str) -> StrId {
        StrId(self.bytes.intern(string.as_bytes()))
    }

    pub fn intern_bytes(&mut self, bytes: &[u8]) -> BytesId {
        BytesId(self.bytes.intern(bytes))
    }

    /// # Safety
    ///
    /// `id` must originate from this interner. A `StrId` minted by a
    /// *different* interner may index content that is not valid UTF-8.
    pub unsafe fn lookup_str(&self, id: StrId) -> &str {
        let bytes = &self.bytes[id.0];
        unsafe { std::str::from_utf8_unchecked(bytes) }
    }

    pub fn lookup_bytes(&self, id: BytesId) -> &[u8] {
        &self.bytes[id.0]
    }
}

impl Default for Interner {
    fn default() -> Self {
        Self::new()
    }
}
