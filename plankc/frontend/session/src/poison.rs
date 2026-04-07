/// Marker: a diagnostic was already emitted; suppress cascading errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Poisoned;

/// A value that is valid or was poisoned by an earlier error.
pub type MaybePoisoned<T> = Result<T, Poisoned>;

mod sealed {
    pub trait Sealed {}
}

pub trait MaybePoisonedResult: sealed::Sealed {
    type I;

    fn zip<B>(self, other: MaybePoisoned<B>) -> MaybePoisoned<(Self::I, B)>;
}

impl<T> sealed::Sealed for MaybePoisoned<T> {}

impl<T> MaybePoisonedResult for MaybePoisoned<T> {
    type I = T;

    fn zip<B>(self, other: MaybePoisoned<B>) -> MaybePoisoned<(T, B)> {
        match (self, other) {
            (Ok(a), Ok(b)) => Ok((a, b)),
            (Err(Poisoned), _) | (_, Err(Poisoned)) => Err(Poisoned),
        }
    }
}
