#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MustUseStrict;

impl MustUseStrict {
    pub fn unchecked_destroy(self) {
        std::mem::forget(self);
    }
}

impl Drop for MustUseStrict {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            panic!("dropped must use type")
        }
    }
}
