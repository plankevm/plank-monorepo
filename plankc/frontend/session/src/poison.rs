/// Marker: a diagnostic was already emitted; suppress cascading errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Poisoned;

/// A value that is valid or was poisoned by an earlier error.
pub type MaybePoisoned<T> = Result<T, Poisoned>;
