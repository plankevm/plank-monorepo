/// Marker: a diagnostic was already emitted; suppress cascading errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Poisoned;

/// A value that is valid or was poisoned by an earlier error.
pub type MaybePoisoned<T> = Result<T, Poisoned>;

pub fn zip<T, B>(a: MaybePoisoned<T>, b: MaybePoisoned<B>) -> MaybePoisoned<(T, B)> {
    match (a, b) {
        (Ok(a), Ok(b)) => Ok((a, b)),
        (Err(Poisoned), _) | (_, Err(Poisoned)) => Err(Poisoned),
    }
}

pub fn transpose<T, E>(p: MaybePoisoned<Result<T, E>>) -> Result<MaybePoisoned<T>, E> {
    match p {
        Ok(Err(err)) => Err(err),
        Err(Poisoned) => Ok(Err(Poisoned)),
        Ok(Ok(ok)) => Ok(Ok(ok)),
    }
}
