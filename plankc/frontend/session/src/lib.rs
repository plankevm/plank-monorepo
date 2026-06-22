pub mod builtins;
pub mod diagnostic;
pub mod display;
mod interner;
pub mod poison;

pub use builtins::{Builtin, RuntimeBuiltin};
pub use diagnostic::*;
pub use display::write_bytes_literal;
pub use interner::{BytesId, EMPTY_BYTES, StrId};
pub use poison::{MaybePoisoned, Poisoned};

use interner::Interner;
use plank_core::{Idx, IndexVec, Span, newtype_index};
use std::path::PathBuf;

newtype_index! {
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
    interner: Interner,
    source_map: IndexVec<SourceId, Source>,
    total_errors: u32,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CBytes {
    pub contents: BytesId,
    pub start: u32,
    pub end: u32,
}

impl CBytes {
    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

impl Session {
    pub fn new() -> Self {
        let mut this = Self {
            interner: Interner::new(),
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
        self.interner.intern_str(name)
    }

    pub fn lookup_name(&self, name: StrId) -> &str {
        // Safety: a session owns exactly one interner; every `StrId` handled
        // by a session originates from it.
        unsafe { self.interner.lookup_str(name) }
    }

    pub fn lookup_name_spanned(&self, name: StrId, start: SourceByteOffset) -> (&str, SourceSpan) {
        let name = self.lookup_name(name);
        (name, Span::new(start, start + name.len() as u32))
    }

    pub fn intern_bytes(&mut self, bytes: &[u8]) -> BytesId {
        self.interner.intern_bytes(bytes)
    }

    pub fn intern_cbytes(&mut self, bytes: &[u8]) -> CBytes {
        let contents = self.intern_bytes(bytes);
        let len = u32::try_from(bytes.len()).expect("cbytes length fits u32");
        CBytes { contents, start: 0, end: len }
    }

    pub fn lookup_bytes(&self, bytes: BytesId) -> &[u8] {
        self.interner.lookup_bytes(bytes)
    }

    pub fn lookup_bytes_slice(&self, bytes: CBytes) -> &[u8] {
        &self.lookup_bytes(bytes.contents)[bytes.start as usize..bytes.end as usize]
    }

    pub fn lookup_bytes_lossy(&self, bytes: CBytes) -> String {
        String::from_utf8_lossy(self.lookup_bytes_slice(bytes)).into_owned()
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
