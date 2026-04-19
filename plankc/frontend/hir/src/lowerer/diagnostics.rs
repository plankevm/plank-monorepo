use plank_parser::lexer::{Token, TokenSpan};
use plank_session::{
    Annotations, Builtin, Claim, ClaimBuilder, Diagnostic, Element, Level, Session, SourceId,
    SourceSpan, StrId,
};

use super::BlockLowerer;

impl BlockLowerer<'_> {
    fn lookup_name(&self, name: StrId) -> String {
        self.session.borrow().lookup_name(name).to_string()
    }

    pub(crate) fn error_not_yet_implemented(&self, feature: &str, span: TokenSpan) {
        let source_span = self.lexed.tokens_src_span(span);
        Diagnostic::error(format!("{feature} is not yet supported"))
            .primary(self.source_id, source_span, "not yet supported")
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_unresolved_identifier(&self, name: StrId, span: TokenSpan) {
        let source_span = self.lexed.tokens_src_span(span);
        let name_str = self.lookup_name(name);
        let at_name = self.session.borrow_mut().intern(&format!("@{name_str}"));
        let mut diagnostic = Diagnostic::error(format!("unresolved identifier '{name_str}'"))
            .primary(self.source_id, source_span, "not found in this scope");
        if let Some(builtin) = Builtin::from_str_id(at_name) {
            diagnostic = diagnostic.help(format!("if you meant the builtin, use `{builtin}`"));
        }
        diagnostic.emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_assignment_to_immutable(
        &self,
        name: StrId,
        span: TokenSpan,
        decl_span: TokenSpan,
    ) {
        let source_span = self.lexed.tokens_src_span(span);
        let decl_source_span = self.lexed.tokens_src_span(decl_span);
        let name_str = self.lookup_name(name);
        Diagnostic::error(format!("variable '{name_str}' was not declared mutable"))
            .element(
                Annotations::new(self.source_id)
                    .primary(source_span, "assignment to immutable variable")
                    .secondary(decl_source_span, "declared here"),
            )
            .help("consider declaring it with `let mut`")
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_multiple_init_blocks(&self, current: TokenSpan, previous: TokenSpan) {
        self.error_multiple_blocks("init", current, previous);
    }

    pub(crate) fn error_multiple_run_blocks(&self, current: TokenSpan, previous: TokenSpan) {
        self.error_multiple_blocks("run", current, previous);
    }

    fn error_multiple_blocks(&self, kind: &str, current: TokenSpan, previous: TokenSpan) {
        Diagnostic::error(format!("multiple {kind} blocks"))
            .element(
                Annotations::new(self.source_id)
                    .primary(self.lexed.tokens_src_span(current), format!("duplicate {kind} block"))
                    .secondary(
                        self.lexed.tokens_src_span(previous),
                        format!("previous {kind} block"),
                    ),
            )
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_init_outside_entry(&self, span: TokenSpan) {
        self.error_outside_entry("init", span);
    }

    pub(crate) fn error_run_outside_entry(&self, span: TokenSpan) {
        self.error_outside_entry("run", span);
    }

    fn error_outside_entry(&self, kind: &str, span: TokenSpan) {
        Diagnostic::error(format!("`{kind}` not allowed here"))
            .primary(
                self.source_id,
                self.lexed.tokens_src_span(span),
                format!("only the entry file may contain `{kind}`"),
            )
            .claim(
                Claim::new(Level::Note, "entry file")
                    .element(Element::Origin { path: SourceId::ROOT }),
            )
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_shadowing_primitive_type(&self, name: StrId, span: TokenSpan) {
        let source_span = self.lexed.tokens_src_span(span);
        let name_str = self.lookup_name(name);
        Diagnostic::error("shadowing primitive type")
            .primary(self.source_id, source_span, format!("'{name_str}' is a primitive type"))
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_unknown_builtin(&self, name: StrId, span: TokenSpan) {
        let source_span = self.lexed.tokens_src_span(span);
        let name_str = self.lookup_name(name);
        Diagnostic::error(format!("unknown builtin '{name_str}'"))
            .primary(self.source_id, source_span, "no built-in function with this name")
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_number_out_of_range(&self, span: TokenSpan) {
        let source_span = self.lexed.tokens_src_span(span);
        Diagnostic::error("number literal out of range")
            .primary(self.source_id, source_span, "value does not fit in u256")
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_non_call_reference_to_builtin(&self, name: StrId, span: TokenSpan) {
        let source_span = self.lexed.tokens_src_span(span);
        let name_str = self.lookup_name(name);
        Diagnostic::error("referencing built-in function as a value")
            .primary(self.source_id, source_span, format!("'{name_str}' is a built-in function"))
            .help("built-in functions must be called directly, wrap in a function if you wish to use it as a first-class value")
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_unresolved_import(
        &self,
        name: StrId,
        span: TokenSpan,
        target_source: SourceId,
    ) {
        let name_str = self.lookup_name(name);
        Diagnostic::error("unresolved import")
            .primary(
                self.source_id,
                self.lexed.tokens_src_span(span),
                format!("'{name_str}' not found in target module"),
            )
            .claim(
                Claim::new(Level::Info, format!("no definition of '{name_str}' found in file"))
                    .element(Element::Origin { path: target_source }),
            )
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_missing_init_block(&self) {
        Diagnostic::error("missing init block")
            .element(Element::Origin { path: SourceId::ROOT })
            .note("the entry file must contain an init block")
            .emit(*self.session.borrow_mut());
    }

    pub(crate) fn error_import_collision(
        &self,
        colliding_name: StrId,
        import_span: TokenSpan,
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
        diagnostic.emit(*self.session.borrow_mut());
    }

    pub fn emit_lone_slash_not_supported(&self, op_span: TokenSpan) {
        let op_span = self.lexed.tokens_src_span(op_span);

        Diagnostic::error("unsupported syntax")
            .primary(self.source_id, op_span, "lone `/` not supported as an operator")
            .help(format!(
                "for division rounding towards 0 use {} (EVM default)",
                Token::LessSlash.name()
            ))
            .help(format!("for division rounding away from 0 use {}", Token::GreaterSlash.name()))
            .help(format!(
                "for division rounding towards negative infinity use {}",
                Token::MinusSlash.name()
            ))
            .help(format!(
                "for division rounding towards positive infinity use {}",
                Token::PlusSlash.name()
            ))
            .emit(*self.session.borrow_mut());
    }

    pub fn emit_return_not_allowed_here(&self, return_span: TokenSpan) {
        Diagnostic::error("return is not allowed outside of function bodies")
            .element(
                Annotations::new(self.source_id)
                    .primary(self.lexed.tokens_src_span(return_span), "not allowed here"),
            )
            .emit(*self.session.borrow_mut());
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
    diagnostic.emit(session);
}
