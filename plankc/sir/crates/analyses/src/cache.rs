use crate::DefUse;
use sir_data::EthIRProgram;

pub trait Analysis {
    fn compute(&mut self, program: &EthIRProgram);
}

pub struct Cached<T: Analysis> {
    inner: T,
    valid: bool,
}

impl<T: Analysis> Cached<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, valid: false }
    }

    pub fn get(&self) -> &T {
        assert!(self.valid, "analysis not valid");
        &self.inner
    }

    pub fn get_if_valid(&self) -> Option<&T> {
        self.valid.then_some(&self.inner)
    }

    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    pub fn mark_valid(&mut self) {
        self.valid = true;
    }

    pub fn ensure(&mut self, program: &EthIRProgram) {
        if !self.valid {
            self.inner.compute(program);
            self.valid = true;
        }
    }
}

macro_rules! define_analyses {
    ($($variant:ident => $field:ident : $ty:ty),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum AnalysisKind {
            $($variant),*
        }

        pub struct AnalysesStore {
            $(pub $field: Cached<$ty>),*
        }

        impl AnalysesStore {
            pub fn new() -> Self {
                Self {
                    $($field: Cached::new(<$ty>::new())),*
                }
            }

            pub fn invalidate(&mut self, kind: AnalysisKind) {
                match kind {
                    $(AnalysisKind::$variant => self.$field.invalidate()),*
                }
            }
        }
    };
}

define_analyses! {
    DefUse => def_use: DefUse,
}
