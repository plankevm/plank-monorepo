use crate::module::ModuleResolveError;
use plank_session::{ClaimBuilder, Diagnostic, Session, SourceId, SourceSpan, StrId};
use std::path::Path;

pub fn error_duplicate_module(session: &mut Session, name: StrId) {
    let name = session.lookup_name(name);
    session.emit_diagnostic(
        Diagnostic::error(format!("duplicate module '{name}'"))
            .help("each module name can only be registered once"),
    );
}

pub fn error_failed_to_canonicalize_entry(
    session: &mut Session,
    path: &Path,
    error: &std::io::Error,
) {
    session.emit_diagnostic(
        Diagnostic::error("could not open entry file")
            .note(format!("'{}': {error}", path.display())),
    );
}

pub fn error_failed_to_resolve_import(
    session: &mut Session,
    source_id: SourceId,
    span: SourceSpan,
    error: &ModuleResolveError,
) {
    let diagnostic =
        match error {
            ModuleResolveError::UnknownModule(name) => {
                let name = session.lookup_name(*name);
                Diagnostic::error("unresolved import").primary(
                    source_id,
                    span,
                    format!("unknown module '{name}'"),
                )
            }
            ModuleResolveError::NotEnoughSegments => Diagnostic::error("unresolved import")
                .primary(source_id, span, "import path is too short"),
        };
    session.emit_diagnostic(diagnostic);
}

pub fn error_failed_to_read_source(session: &mut Session, path: &Path, error: &std::io::Error) {
    session.emit_diagnostic(
        Diagnostic::error("could not read source file")
            .note(format!("'{}': {error}", path.display())),
    );
}

pub fn error_failed_to_canonicalize_import(
    session: &mut Session,
    source_id: SourceId,
    span: SourceSpan,
    path: &Path,
    error: &std::io::Error,
) {
    session.emit_diagnostic(
        Diagnostic::error("could not open imported file")
            .primary(source_id, span, "imported here")
            .note(format!("'{}': {error}", path.display())),
    );
}
