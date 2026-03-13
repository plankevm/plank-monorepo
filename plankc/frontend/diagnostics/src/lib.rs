use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use plank_core::{IndexVec, SourceId, SourceSpan};
use plank_source::project::Source;

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

impl From<Severity> for Level<'static> {
    fn from(severity: Severity) -> Self {
        match severity {
            Severity::Error => Level::ERROR,
            Severity::Warning => Level::WARNING,
        }
    }
}

impl From<AnnotationStyle> for AnnotationKind {
    fn from(style: AnnotationStyle) -> Self {
        match style {
            AnnotationStyle::Primary => AnnotationKind::Primary,
            AnnotationStyle::Secondary => AnnotationKind::Context,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterKind {
    Note,
    Help,
}

impl From<FooterKind> for Level<'static> {
    fn from(kind: FooterKind) -> Self {
        match kind {
            FooterKind::Note => Level::NOTE,
            FooterKind::Help => Level::HELP,
        }
    }
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

impl SimpleCollector {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
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
    pub fn error(message: impl Into<String>) -> Self {
        let message = message.into();
        Self { severity: Severity::Error, message, annotations: Vec::new(), footers: Vec::new() }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        let message = message.into();
        Self { severity: Severity::Warning, message, annotations: Vec::new(), footers: Vec::new() }
    }

    pub fn primary(
        mut self,
        source_id: SourceId,
        span: SourceSpan,
        label: impl Into<String>,
    ) -> Self {
        self.annotations.push(Annotation {
            source_id,
            span,
            label: Some(label.into()),
            style: AnnotationStyle::Primary,
        });
        self
    }

    pub fn secondary(
        mut self,
        source_id: SourceId,
        span: SourceSpan,
        label: impl Into<String>,
    ) -> Self {
        self.annotations.push(Annotation {
            source_id,
            span,
            label: Some(label.into()),
            style: AnnotationStyle::Secondary,
        });
        self
    }

    pub fn span(mut self, source_id: SourceId, span: SourceSpan, style: AnnotationStyle) -> Self {
        self.annotations.push(Annotation { source_id, span, label: None, style });
        self
    }

    pub fn note(mut self, message: impl Into<String>) -> Self {
        self.footers.push(Footer { kind: FooterKind::Note, message: message.into() });
        self
    }

    pub fn help(mut self, message: impl Into<String>) -> Self {
        self.footers.push(Footer { kind: FooterKind::Help, message: message.into() });
        self
    }

    pub fn render(&self, sources: &IndexVec<SourceId, Source>) -> String {
        self.render_with(sources, Renderer::plain())
    }

    pub fn render_styled(&self, sources: &IndexVec<SourceId, Source>) -> String {
        self.render_with(sources, Renderer::styled())
    }

    fn render_with(&self, sources: &IndexVec<SourceId, Source>, renderer: Renderer) -> String {
        let title = Level::from(self.severity).primary_title(&self.message);

        let mut seen_sources: Vec<SourceId> = Vec::new();
        for ann in &self.annotations {
            if !seen_sources.contains(&ann.source_id) {
                seen_sources.push(ann.source_id);
            }
        }

        let mut snippets: Vec<Snippet<'_, annotate_snippets::Annotation<'_>>> = Vec::new();
        for &source_id in &seen_sources {
            let source = &sources[source_id];
            let path = source.path.to_str().expect("source path is not valid UTF-8");
            let mut snippet = Snippet::source(&source.content).path(path);
            for ann in self.annotations.iter().filter(|a| a.source_id == source_id) {
                let marker = AnnotationKind::from(ann.style).span(ann.span.usize_range());
                snippet = snippet.annotation(match &ann.label {
                    Some(label) => marker.label(label),
                    None => marker,
                });
            }
            snippets.push(snippet);
        }

        let mut group = title.elements(snippets);

        for footer in &self.footers {
            group = group.element(Level::from(footer.kind).message(&footer.message));
        }

        renderer.render(&[group])
    }
}

pub trait DiagnosticsContext {
    fn emit(&mut self, diagnostic: Diagnostic);
}
