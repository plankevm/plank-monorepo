pub mod builtins;
pub mod diagnostic;
pub mod poison;
pub mod types;

pub use builtins::EvmBuiltin;
pub use diagnostic::*;
pub use poison::{MaybePoisoned, Poisoned};
pub use types::TypeId;

use plank_core::{Idx, IndexVec, Span, intern::StringInterner, newtype_index};
use std::path::PathBuf;

newtype_index! {
    pub struct StrId;
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
    name_interner: StringInterner<StrId>,
    source_map: IndexVec<SourceId, Source>,
    diagnostics: Vec<Diagnostic>,
}

impl Session {
    pub fn new() -> Self {
        let mut this = Self {
            name_interner: StringInterner::new(),
            source_map: IndexVec::new(),
            diagnostics: Vec::new(),
        };
        builtins::inject_builtins(&mut this);
        this
    }

    pub fn intern(&mut self, name: &str) -> StrId {
        self.name_interner.intern(name)
    }

    pub fn lookup_name(&self, name: StrId) -> &str {
        &self.name_interner[name]
    }

    pub fn lookup_name_spanned(&self, name: StrId, start: SourceByteOffset) -> (&str, SourceSpan) {
        let name = &self.name_interner[name];
        (name, Span::new(start, start + name.len() as u32))
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

    pub fn emit_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error())
    }

    pub fn interner(&self) -> &plank_core::intern::StringInterner<StrId> {
        &self.name_interner
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

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
