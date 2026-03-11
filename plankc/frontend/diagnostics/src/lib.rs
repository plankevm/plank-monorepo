use plank_core::{SourceId, SourceSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationStyle {
    Primary,
    Secondary,
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub source_id: SourceId,
    pub span: SourceSpan,
    pub label: Option<String>,
    pub style: AnnotationStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterKind {
    Note,
    Help,
}

#[derive(Debug, Clone)]
pub struct Footer {
    pub kind: FooterKind,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct SimpleCollector {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticsContext for SimpleCollector {
    fn emit(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub annotations: Vec<Annotation>,
    pub footers: Vec<Footer>,
}

impl Diagnostic {
    pub fn error(message: String) -> Self {
        Self { severity: Severity::Error, message, annotations: Vec::new(), footers: Vec::new() }
    }

    pub fn warning(message: String) -> Self {
        Self { severity: Severity::Warning, message, annotations: Vec::new(), footers: Vec::new() }
    }

    pub fn primary(mut self, source_id: SourceId, span: SourceSpan, label: String) -> Self {
        self.annotations.push(Annotation {
            source_id,
            span,
            label: Some(label),
            style: AnnotationStyle::Primary,
        });
        self
    }

    pub fn secondary(mut self, source_id: SourceId, span: SourceSpan, label: String) -> Self {
        self.annotations.push(Annotation {
            source_id,
            span,
            label: Some(label),
            style: AnnotationStyle::Secondary,
        });
        self
    }

    pub fn span(mut self, source_id: SourceId, span: SourceSpan, style: AnnotationStyle) -> Self {
        self.annotations.push(Annotation { source_id, span, label: None, style });
        self
    }

    pub fn note(mut self, message: String) -> Self {
        self.footers.push(Footer { kind: FooterKind::Note, message });
        self
    }

    pub fn help(mut self, message: String) -> Self {
        self.footers.push(Footer { kind: FooterKind::Help, message });
        self
    }
}

pub trait DiagnosticsContext {
    fn emit(&mut self, diagnostic: Diagnostic);
}
