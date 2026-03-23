use plank_session::{Diagnostic, Session, StrId};

pub fn error_duplicate_module(session: &mut Session, name: StrId) {
    let name = session.lookup_name(name);
    session.emit_diagnostic(
        Diagnostic::error(format!("duplicate module '{name}'"))
            .help("each module name can only be registered once"),
    );
}
