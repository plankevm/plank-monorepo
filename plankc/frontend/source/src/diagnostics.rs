use crate::module::ModuleResolveError;
use plank_session::{ClaimBuilder, Diagnostic, Session, SourceId, SourceSpan, StrId};
use std::path::Path;

pub fn error_duplicate_module(session: &mut Session, name: StrId) {
    let name = session.lookup_name(name);
    Diagnostic::error(format!("duplicate module '{name}'"))
        .help("each module name can only be registered once")
        .emit(session);
}

pub fn error_failed_to_canonicalize_entry(
    session: &mut Session,
    path: &Path,
    error: &std::io::Error,
) {
    Diagnostic::error("could not open entry file")
        .note(format!("'{}': {error}", path.display()))
        .emit(session);
}

pub fn error_failed_to_resolve_import(
    session: &mut Session,
    source_id: SourceId,
    span: SourceSpan,
    error: &ModuleResolveError,
) {
    match error {
        ModuleResolveError::UnknownModule(name) => {
            let name = session.lookup_name(*name);
            let mut diagnostic = Diagnostic::error("unresolved import").primary(
                source_id,
                span,
                format!("unknown module '{name}'"),
            );
            if name == "std" {
                diagnostic = diagnostic
                    .help("the 'std' module is included with plankup, the Plank installer")
                    .note("see https://github.com/plankevm/plank-monorepo for installation instructions");
            }
            diagnostic.emit(session)
        }
        ModuleResolveError::NotEnoughSegments => Diagnostic::error("unresolved import")
            .primary(source_id, span, "import path is too short")
            .emit(session),
    };
}

pub fn error_failed_to_read_source(session: &mut Session, path: &Path, error: &std::io::Error) {
    Diagnostic::error("could not read source file")
        .note(format!("'{}': {error}", path.display()))
        .emit(session);
}

pub fn error_failed_to_canonicalize_import(
    session: &mut Session,
    source_id: SourceId,
    span: SourceSpan,
    path: &Path,
    error: &std::io::Error,
) {
    Diagnostic::error("could not open imported file")
        .primary(source_id, span, "imported here")
        .note(format!("'{}': {error}", path.display()))
        .emit(session);
}
