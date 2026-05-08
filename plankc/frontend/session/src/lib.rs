pub mod builtins;
pub mod diagnostic;
pub mod poison;

pub use builtins::{Builtin, RuntimeBuiltin};
pub use diagnostic::*;
pub use poison::{MaybePoisoned, Poisoned};

use plank_core::{Idx, IndexVec, Span, intern::BytesInterner, newtype_index};
use std::path::PathBuf;

newtype_index! {
    pub struct StrId;
    pub struct BytesId;
    pub struct SourceId;
    pub struct SourceByteOffset;
}

impl SourceId {
    pub const ROOT: Self = Self::new(0);
}

pub type SourceSpan = Span<SourceByteOffset>;
pub const ZERO_SPAN: SourceSpan = Span::new(SourceByteOffset::ZERO, SourceByteOffset::ZERO);

#[derive(Debug, Clone)]
pub struct Source {
    pub path: PathBuf,
    pub content: String,
}

pub struct Session {
    bytes_interner: BytesInterner<BytesId>,
    source_map: IndexVec<SourceId, Source>,
    total_errors: u32,
    diagnostics: Vec<Diagnostic>,
}

impl From<StrId> for BytesId {
    fn from(value: StrId) -> Self {
        BytesId::from_raw(value.to_raw())
    }
}

impl Session {
    pub const EMPTY_STRING: StrId = StrId::new(0);

    pub fn new() -> Self {
        let mut bytes_interner = BytesInterner::new();
        assert_eq!(bytes_interner.intern(b""), BytesId::from(Self::EMPTY_STRING));
        let mut this = Self {
            bytes_interner,
            source_map: IndexVec::new(),
            total_errors: 0,
            diagnostics: Vec::new(),
        };
        builtins::inject_builtins(&mut this);
        this
    }

    pub fn total_errors(&self) -> u32 {
        self.total_errors
    }

    pub fn intern(&mut self, name: &str) -> StrId {
        let bytes_id = self.bytes_interner.intern(name.as_bytes());
        StrId::from_raw(bytes_id.to_raw())
    }

    pub fn lookup_name(&self, name: StrId) -> &str {
        let as_bytes_id = BytesId::from_raw(name.to_raw());
        unsafe { core::str::from_utf8_unchecked(&self.bytes_interner[as_bytes_id]) }
    }

    pub fn lookup_name_spanned(&self, name: StrId, start: SourceByteOffset) -> (&str, SourceSpan) {
        let name = self.lookup_name(name);
        (name, Span::new(start, start + name.len() as u32))
    }

    pub fn intern_bytes(&mut self, bytes: &[u8]) -> BytesId {
        self.bytes_interner.intern(bytes)
    }

    pub fn lookup_bytes(&self, bytes: BytesId) -> &[u8] {
        &self.bytes_interner[bytes]
    }

    pub fn lookup_bytes_slice(&self, bytes: BytesId, start: u32, end: u32) -> &[u8] {
        &self.lookup_bytes(bytes)[start as usize..end as usize]
    }

    pub fn lookup_bytes_lossy(&self, bytes: BytesId, start: u32, end: u32) -> String {
        String::from_utf8_lossy(self.lookup_bytes_slice(bytes, start, end)).into_owned()
    }

    pub fn next_source(&self) -> SourceId {
        self.source_map.next_idx()
    }

    pub fn register_source(&mut self, source: Source) -> SourceId {
        self.source_map.push(source)
    }

    pub fn get_source(&self, source: SourceId) -> &Source {
        &self.source_map[source]
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn has_errors(&self) -> bool {
        self.total_errors() > 0
    }

    /// Both line and col are 1-indexed. O(n) linear scan.
    pub fn offset_to_line_col(&self, source_id: SourceId, offset: SourceByteOffset) -> (u32, u32) {
        let source = self.get_source(source_id);
        let byte_offset = offset.idx();
        let mut line: u32 = 1;
        let mut col: u32 = 1;
        for (i, ch) in source.content.char_indices() {
            if i >= byte_offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }
}

impl DiagEmitter for Session {
    fn emit_diagnostic(&mut self, diagnostic: Diagnostic) {
        if diagnostic.level == Level::Error {
            self.total_errors += 1;
        }
        self.diagnostics.push(diagnostic);
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
