#[derive(Default)]
pub struct VecBuf<T>(Vec<T>);

impl<T> VecBuf<T> {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn use_as<R>(&mut self, f: impl FnOnce(&mut Vec<T>) -> R) -> R {
        let res = f(&mut self.0);
        self.0.clear();
        res
    }
}
