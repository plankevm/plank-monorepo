use crate::{
    BasicBlockOwnershipAndReachability, ControlFlowGraphInOutBundling, DefUse, DominanceFrontiers,
    Dominators, Predecessors,
};
use sir_data::EthIRProgram;

pub struct Cached<T> {
    inner: T,
    valid: bool,
}

impl<T> Cached<T> {
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
    Predecessors => predecessors: Predecessors,
    Dominators => dominators: Dominators,
    DominanceFrontiers => dominance_frontiers: DominanceFrontiers,
    BasicBlockOwnership => basic_block_ownership: BasicBlockOwnershipAndReachability,
    CfgInOutBundling => cfg_in_out_bundling: ControlFlowGraphInOutBundling,
}

impl AnalysesStore {
    pub fn ensure(&mut self, kind: AnalysisKind, program: &EthIRProgram) {
        match kind {
            AnalysisKind::DefUse => {
                if !self.def_use.valid {
                    self.def_use.inner.compute(program);
                    self.def_use.valid = true;
                }
            }
            AnalysisKind::Predecessors => {
                if !self.predecessors.valid {
                    self.predecessors.inner.compute(program);
                    self.predecessors.valid = true;
                }
            }
            AnalysisKind::Dominators => {
                if !self.dominators.valid {
                    self.ensure(AnalysisKind::Predecessors, program);
                    self.dominators.inner.compute(program, self.predecessors.get());
                    self.dominators.valid = true;
                }
            }
            AnalysisKind::DominanceFrontiers => {
                if !self.dominance_frontiers.valid {
                    self.ensure(AnalysisKind::Dominators, program);
                    self.dominance_frontiers
                        .inner
                        .compute(self.dominators.get(), self.predecessors.get());
                    self.dominance_frontiers.valid = true;
                }
            }
            AnalysisKind::BasicBlockOwnership => {
                if !self.basic_block_ownership.valid {
                    self.basic_block_ownership.inner.compute(program);
                    self.basic_block_ownership.valid = true;
                }
            }
            AnalysisKind::CfgInOutBundling => {
                if !self.cfg_in_out_bundling.valid {
                    self.cfg_in_out_bundling.inner.compute(program);
                    self.cfg_in_out_bundling.valid = true;
                }
            }
        }
    }
}
