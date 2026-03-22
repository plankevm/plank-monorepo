use plank_core::Span;
use plank_parser::lexer::TokenIdx;
use plank_session::{
    Annotations, Claim, Diagnostic, Element, Level, Session, SourceId, SourceSpan, StrId,
};

use super::BlockLowerer;

impl BlockLowerer<'_> {
    pub(crate) fn emit_diagnostic(&self, diagnostic: Diagnostic) {
        self.session.borrow_mut().emit_diagnostic(diagnostic);
    }

    fn lookup_name(&self, name: StrId) -> String {
        self.session.borrow().lookup_name(name).to_string()
    }

    pub(crate) fn error_not_yet_implemented(&self, feature: &str, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let diagnostic = Diagnostic::error(format!("{feature} is not yet supported")).primary(
            self.source_id,
            source_span,
            "not yet supported",
        );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_unresolved_identifier(&self, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let name_str = self.lookup_name(name);
        let diagnostic = Diagnostic::error(format!("unresolved identifier '{name_str}'")).primary(
            self.source_id,
            source_span,
            "not found in this scope",
        );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_assignment_to_immutable(
        &self,
        name: StrId,
        span: Span<TokenIdx>,
        decl_span: Span<TokenIdx>,
    ) {
        let source_span = self.lexed.tokens_src_span(span);
        let decl_source_span = self.lexed.tokens_src_span(decl_span);
        let name_str = self.lookup_name(name);
        let diagnostic =
            Diagnostic::error(format!("variable '{name_str}' was not declared mutable"))
                .element(
                    Annotations::new(self.source_id)
                        .primary(source_span, "assignment to immutable variable")
                        .secondary(decl_source_span, "declared here"),
                )
                .help("consider declaring it with `let mut`");
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_multiple_init_blocks(
        &self,
        current: Span<TokenIdx>,
        previous: Span<TokenIdx>,
    ) {
        self.error_multiple_blocks("init", current, previous);
    }

    pub(crate) fn error_multiple_run_blocks(
        &self,
        current: Span<TokenIdx>,
        previous: Span<TokenIdx>,
    ) {
        self.error_multiple_blocks("run", current, previous);
    }

    fn error_multiple_blocks(&self, kind: &str, current: Span<TokenIdx>, previous: Span<TokenIdx>) {
        let diagnostic = Diagnostic::error(format!("multiple {kind} blocks")).element(
            Annotations::new(self.source_id)
                .primary(self.lexed.tokens_src_span(current), format!("duplicate {kind} block"))
                .secondary(self.lexed.tokens_src_span(previous), format!("previous {kind} block")),
        );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_init_outside_entry(&self, span: Span<TokenIdx>) {
        self.error_outside_entry("init", span);
    }

    pub(crate) fn error_run_outside_entry(&self, span: Span<TokenIdx>) {
        self.error_outside_entry("run", span);
    }

    fn error_outside_entry(&self, kind: &str, span: Span<TokenIdx>) {
        let diagnostic = Diagnostic::error(format!("`{kind}` not allowed here"))
            .primary(
                self.source_id,
                self.lexed.tokens_src_span(span),
                format!("only the entry file may contain `{kind}`"),
            )
            .add_claim(
                Claim::new(Level::Note, "entry file")
                    .element(Element::Origin { path: SourceId::ROOT }),
            );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_shadowing_primitive_type(&self, name: StrId, span: Span<TokenIdx>) {
        self.error_shadowing("primitive type", name, span);
    }

    pub(crate) fn error_shadowing_builtin(&self, name: StrId, span: Span<TokenIdx>) {
        self.error_shadowing("built-in function", name, span);
    }

    fn error_shadowing(&self, kind: &str, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let name_str = self.lookup_name(name);
        let diagnostic = Diagnostic::error(format!("shadowing {kind}")).primary(
            self.source_id,
            source_span,
            format!("'{name_str}' is a {kind}"),
        );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_number_out_of_range(&self, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let diagnostic = Diagnostic::error("number literal out of range").primary(
            self.source_id,
            source_span,
            "value does not fit in u256",
        );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_non_call_reference_to_builtin(&self, name: StrId, span: Span<TokenIdx>) {
        let source_span = self.lexed.tokens_src_span(span);
        let name_str = self.lookup_name(name);
        let diagnostic = Diagnostic::error("referencing built-in function as a value")
            .primary(self.source_id, source_span, format!("'{name_str}' is a built-in function"))
            .help("built-in functions must be called directly, wrap in a function if you wish to use it as a first-class value");
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_unresolved_import(
        &self,
        name: StrId,
        span: Span<TokenIdx>,
        target_source: SourceId,
    ) {
        let name_str = self.lookup_name(name);
        let diagnostic = Diagnostic::error("unresolved import")
            .primary(
                self.source_id,
                self.lexed.tokens_src_span(span),
                format!("'{name_str}' not found in target module"),
            )
            .add_claim(
                Claim::new(Level::Info, format!("no definition of '{name_str}' found in file"))
                    .element(Element::Origin { path: target_source }),
            );
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_missing_init_block(&self) {
        let diagnostic = Diagnostic::error("missing init block")
            .element(Element::Origin { path: SourceId::ROOT })
            .note("the entry file must contain an init block");
        self.emit_diagnostic(diagnostic);
    }

    pub(crate) fn error_import_collision(
        &self,
        colliding_name: StrId,
        import_span: Span<TokenIdx>,
        prev_source_id: SourceId,
        prev_source_span: SourceSpan,
        prev_imported: bool,
        glob_def: Option<(SourceId, SourceSpan)>,
    ) {
        let name_str = self.lookup_name(colliding_name);
        let prev_label = if prev_imported {
            format!("'{name_str}' previously imported here")
        } else {
            format!("'{name_str}' previously defined here")
        };
        let source_span = self.lexed.tokens_src_span(import_span);
        let mut diagnostic = Diagnostic::error("imported definition collision");
        if self.source_id == prev_source_id {
            diagnostic = diagnostic.element(
                Annotations::new(self.source_id)
                    .primary(source_span, "conflicting import")
                    .secondary(prev_source_span, prev_label),
            );
        } else {
            diagnostic = diagnostic
                .primary(self.source_id, source_span, "conflicting import")
                .element(Annotations::new(prev_source_id).secondary(prev_source_span, prev_label));
        }
        if let Some((def_source_id, def_span)) = glob_def {
            diagnostic = diagnostic.element(
                Annotations::new(def_source_id)
                    .secondary(def_span, format!("imported colliding '{name_str}'")),
            );
        }
        self.emit_diagnostic(diagnostic);
    }
}

pub(super) fn error_duplicate_const(
    session: &mut Session,
    source_id: SourceId,
    name: StrId,
    source_span: SourceSpan,
    prev: &crate::ConstDef,
) {
    let name = session.lookup_name(name);
    let mut diagnostic = Diagnostic::error(format!("duplicate definition of '{name}'"));
    if source_id == prev.source_id {
        diagnostic = diagnostic.element(
            Annotations::new(source_id)
                .primary(source_span, format!("'{name}' redefined here"))
                .secondary(prev.source_span, "previously defined here"),
        );
    } else {
        diagnostic =
            diagnostic.primary(source_id, source_span, format!("'{name}' redefined here")).element(
                Annotations::new(prev.source_id)
                    .secondary(prev.source_span, "previously defined here"),
            );
    }
    session.emit_diagnostic(diagnostic);
}
