//! Wrapper around `annotate_snippets` that defers resolution of paths and sources by capturing the
//! corresponding `SourceId`.

use crate::{Session, SourceId, SourceSpan};
use annotate_snippets as snip;
use annotate_snippets::{Group, Renderer, Snippet};

pub use annotate_snippets::AnnotationKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    Info,
    Note,
    Help,
}

impl From<Level> for snip::Level<'static> {
    fn from(value: Level) -> Self {
        match value {
            Level::Error => snip::Level::ERROR,
            Level::Warning => snip::Level::WARNING,
            Level::Info => snip::Level::INFO,
            Level::Note => snip::Level::NOTE,
            Level::Help => snip::Level::HELP,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub span: SourceSpan,
    pub label: Option<String>,
    pub kind: AnnotationKind,
}

#[derive(Debug, Clone)]
pub struct Patch {
    pub span: SourceSpan,
    pub replacement: String,
}

#[derive(Debug, Clone)]
pub enum Element {
    /// `level: None` suppresses the level name prefix in the rendered output.
    Message {
        level: Option<Level>,
        text: String,
    },
    Annotations(Annotations),
    Patches(Patches),
    Origin {
        path: SourceId,
    },
}

#[derive(Debug, Clone)]
pub struct Patches {
    source: SourceId,
    patches: Vec<Patch>,
}

#[derive(Debug, Clone)]
pub struct Annotations {
    source: SourceId,
    annotations: Vec<Annotation>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct SrcLoc {
    pub source: SourceId,
    pub span: SourceSpan,
}

impl SrcLoc {
    pub fn new(source: SourceId, span: SourceSpan) -> Self {
        Self { source, span }
    }
}

impl Annotations {
    pub fn new(source: SourceId) -> Self {
        Self { source, annotations: Vec::new() }
    }

    pub fn primary(mut self, span: SourceSpan, label: impl Into<String>) -> Self {
        self.annotations.push(Annotation {
            span,
            label: Some(label.into()),
            kind: AnnotationKind::Primary,
        });
        self
    }

    pub fn secondary(mut self, span: SourceSpan, label: impl Into<String>) -> Self {
        self.annotations.push(Annotation {
            span,
            label: Some(label.into()),
            kind: AnnotationKind::Context,
        });
        self
    }

    pub fn no_label(mut self, span: SourceSpan, kind: AnnotationKind) -> Self {
        self.annotations.push(Annotation { span, label: None, kind });
        self
    }
}

impl Patches {
    pub fn new(source: SourceId) -> Self {
        Self { source, patches: Vec::new() }
    }

    pub fn lone(source: SourceId, span: SourceSpan, replacement: impl Into<String>) -> Self {
        Self::new(source).patch(span, replacement)
    }

    pub fn patch(mut self, span: SourceSpan, replacement: impl Into<String>) -> Self {
        self.patches.push(Patch { span, replacement: replacement.into() });
        self
    }
}

impl From<Annotations> for Element {
    fn from(cause: Annotations) -> Self {
        Element::Annotations(cause)
    }
}

impl From<Patches> for Element {
    fn from(value: Patches) -> Self {
        Element::Patches(value)
    }
}

#[derive(Debug, Clone)]
pub struct Claim {
    pub level: Level,
    pub title: String,
    pub elements: Vec<Element>,
}

impl Claim {
    pub fn new(level: Level, title: impl Into<String>) -> Self {
        Self { level, title: title.into(), elements: Vec::new() }
    }

    pub fn element(mut self, element: impl Into<Element>) -> Self {
        self.elements.push(element.into());
        self
    }

    pub fn primary(self, source_id: SourceId, span: SourceSpan, label: impl Into<String>) -> Self {
        self.element(Annotations::new(source_id).primary(span, label))
    }

    pub fn note(mut self, message: impl Into<String>) -> Self {
        self.elements.push(Element::Message { level: Some(Level::Note), text: message.into() });
        self
    }

    pub fn help(mut self, message: impl Into<String>) -> Self {
        self.elements.push(Element::Message { level: Some(Level::Help), text: message.into() });
        self
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: Level,
    pub title: String,
    pub primary_elements: Vec<Element>,
    pub added_claims: Vec<Claim>,
}

impl Diagnostic {
    pub fn is_error(&self) -> bool {
        self.level == Level::Error
    }

    pub fn error(title: impl Into<String>) -> Self {
        Self {
            level: Level::Error,
            title: title.into(),
            primary_elements: Vec::new(),
            added_claims: Vec::new(),
        }
    }

    pub fn warning(title: impl Into<String>) -> Self {
        Self {
            level: Level::Warning,
            title: title.into(),
            primary_elements: Vec::new(),
            added_claims: Vec::new(),
        }
    }

    pub fn element(mut self, element: impl Into<Element>) -> Self {
        self.primary_elements.push(element.into());
        self
    }

    pub fn primary(self, source_id: SourceId, span: SourceSpan, label: impl Into<String>) -> Self {
        self.element(Annotations::new(source_id).primary(span, label))
    }

    pub fn cross_source_annotations(
        self,
        primary_loc: SrcLoc,
        primary_label: impl Into<String>,
        secondary_loc: SrcLoc,
        secondary_label: impl Into<String>,
    ) -> Self {
        if primary_loc.source == secondary_loc.source {
            self.element(
                Annotations::new(primary_loc.source)
                    .primary(primary_loc.span, primary_label)
                    .secondary(secondary_loc.span, secondary_label),
            )
        } else {
            self.primary(primary_loc.source, primary_loc.span, primary_label).element(
                Annotations::new(secondary_loc.source)
                    .secondary(secondary_loc.span, secondary_label),
            )
        }
    }

    pub fn note(mut self, message: impl Into<String>) -> Self {
        self.primary_elements
            .push(Element::Message { level: Some(Level::Note), text: message.into() });
        self
    }

    pub fn help(mut self, message: impl Into<String>) -> Self {
        self.primary_elements
            .push(Element::Message { level: Some(Level::Help), text: message.into() });
        self
    }

    pub fn add_claim(mut self, claim: Claim) -> Self {
        self.added_claims.push(claim);
        self
    }

    pub fn render_plain(&self, session: &Session) -> String {
        self.render_with(session, Renderer::plain())
    }

    pub fn render_styled(&self, session: &Session) -> String {
        self.render_with(session, Renderer::styled())
    }

    fn render_with(&self, session: &Session, renderer: Renderer) -> String {
        let mut groups: Vec<Group<'_>> = Vec::new();

        let title = snip::Level::from(self.level).primary_title(&self.title);
        groups.push(Self::build_group(title, &self.primary_elements, session));

        for claim in &self.added_claims {
            let claim_title = snip::Level::from(claim.level).secondary_title(&claim.title);
            groups.push(Self::build_group(claim_title, &claim.elements, session));
        }

        renderer.render(&groups)
    }

    fn build_group<'a>(
        title: snip::Title<'a>,
        elements: &'a [Element],
        session: &'a Session,
    ) -> Group<'a> {
        let mut group = Group::with_title(title);

        for element in elements {
            match element {
                Element::Message { level: Some(l), text } => {
                    group = group.element(snip::Level::from(*l).message(text.as_str()));
                }
                Element::Message { level: None, text } => {
                    group = group.element(snip::Level::NOTE.no_name().message(text.as_str()));
                }
                Element::Annotations(cause) => {
                    let src = session.get_source(cause.source);
                    let path = src.path.to_str().expect("source path is not valid UTF-8");
                    let mut snippet: Snippet<'_, snip::Annotation<'_>> =
                        Snippet::source(&src.content).path(path);
                    for ann in &cause.annotations {
                        let marker = ann.kind.span(ann.span.usize_range());
                        snippet = snippet.annotation(match &ann.label {
                            Some(label) => marker.label(label.as_str()),
                            None => marker,
                        });
                    }
                    group = group.element(snippet);
                }
                Element::Patches(patches) => {
                    let src = session.get_source(patches.source);
                    let path = src.path.to_str().expect("source path is not valid UTF-8");
                    let mut snippet: Snippet<'_, snip::Patch<'_>> =
                        Snippet::source(&src.content).path(path);
                    for p in &patches.patches {
                        snippet =
                            snippet.patch(snip::Patch::new(p.span.usize_range(), &*p.replacement));
                    }
                    group = group.element(snippet);
                }
                Element::Origin { path: source_id } => {
                    let src = session.get_source(*source_id);
                    let path = src.path.to_str().expect("source path is not valid UTF-8");
                    group = group.element(snip::Origin::path(path));
                }
            }
        }

        group
    }
}
