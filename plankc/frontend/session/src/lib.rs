pub mod builtins;
pub mod diagnostic;
pub mod types;

pub use builtins::Builtin;
pub use diagnostic::Diagnostic;
pub use types::TypeId;

use plank_core::{IndexVec, Span, intern::StringInterner, newtype_index};
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
        self.diagnostics.iter().any(|d| d.severity == diagnostic::Severity::Error)
    }

    pub fn interner(&self) -> &plank_core::intern::StringInterner<StrId> {
        &self.name_interner
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
