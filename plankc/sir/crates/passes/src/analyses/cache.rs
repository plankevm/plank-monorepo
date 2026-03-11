use crate::{
    BasicBlockOwnershipAndReachability, ControlFlowGraphInOutBundling, DefUse, DominanceFrontiers,
    Dominators, Predecessors,
};
use sir_data::EthIRProgram;

#[derive(Default)]
pub struct Cached<T> {
    inner: T,
    valid: bool,
}

impl<T> Cached<T> {
    pub fn new(inner: T) -> Self {
        Self { inner, valid: false }
    }

    fn get(&self) -> &T {
        assert!(self.valid, "analysis not valid");
        &self.inner
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    pub fn get_if_valid(&self) -> Option<&T> {
        self.valid.then_some(&self.inner)
    }

    pub fn invalidate(&mut self) {
        self.valid = false;
    }

    pub fn update(&mut self, f: impl FnOnce(&mut T)) {
        f(&mut self.inner);
        self.valid = true;
    }
}

macro_rules! define_analyses {
    ($($variant:ident => $field:ident : $ty:ty [used_by: $($dep:ident),*]),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum AnalysisKind {
            $($variant),*
        }

        impl AnalysisKind {
            fn used_by(&self) -> &[AnalysisKind] {
                match self {
                    $(AnalysisKind::$variant => &[$(AnalysisKind::$dep),*]),*
                }
            }
        }

        #[derive(Default)]
        pub struct AnalysesStore {
            $($field: Cached<$ty>),*
        }

        impl AnalysesStore {
            pub fn is_valid(&self, kind: AnalysisKind) -> bool {
                match kind {
                    $(AnalysisKind::$variant => self.$field.is_valid()),*
                }
            }

            pub fn invalidate(&mut self, kind: AnalysisKind) {
                match kind {
                    $(AnalysisKind::$variant => self.$field.invalidate()),*
                }
                for dependent in kind.used_by() {
                    self.invalidate(*dependent);
                }
            }
        }
    };
}

define_analyses! {
    DefUse => def_use: DefUse [used_by:],
    // DominanceFrontiers also depends on Predecessors but is transitively
    // invalidated via Dominators.
    Predecessors => predecessors: Predecessors [used_by: Dominators],
    Dominators => dominators: Dominators [used_by: DominanceFrontiers],
    DominanceFrontiers => dominance_frontiers: DominanceFrontiers [used_by:],
    BasicBlockOwnership => basic_block_ownership: BasicBlockOwnershipAndReachability [used_by:],
    CfgInOutBundling => cfg_in_out_bundling: ControlFlowGraphInOutBundling [used_by:],
}

impl AnalysesStore {
    pub fn def_use(&mut self, program: &EthIRProgram) -> &DefUse {
        if !self.def_use.valid {
            self.def_use.inner.compute(program);
            self.def_use.valid = true;
        }
        &self.def_use.inner
    }

    pub fn def_use_mut(&mut self, program: &EthIRProgram) -> &mut DefUse {
        if !self.def_use.valid {
            self.def_use.inner.compute(program);
        }
        self.def_use.valid = false;
        &mut self.def_use.inner
    }

    pub fn predecessors(&mut self, program: &EthIRProgram) -> &Predecessors {
        if !self.predecessors.valid {
            self.predecessors.inner.compute(program);
            self.predecessors.valid = true;
        }
        &self.predecessors.inner
    }

    pub fn dominators(&mut self, program: &EthIRProgram) -> &Dominators {
        self.predecessors(program);
        if !self.dominators.valid {
            self.dominators.inner.compute(program, self.predecessors.get());
            self.dominators.valid = true;
        }
        &self.dominators.inner
    }

    pub fn dominance_frontiers(&mut self, program: &EthIRProgram) -> &DominanceFrontiers {
        self.dominators(program);
        if !self.dominance_frontiers.valid {
            self.dominance_frontiers.inner.compute(self.dominators.get(), self.predecessors.get());
            self.dominance_frontiers.valid = true;
        }
        &self.dominance_frontiers.inner
    }

    pub fn basic_block_ownership(
        &mut self,
        program: &EthIRProgram,
    ) -> &BasicBlockOwnershipAndReachability {
        if !self.basic_block_ownership.valid {
            self.basic_block_ownership.inner.compute(program);
            self.basic_block_ownership.valid = true;
        }
        &self.basic_block_ownership.inner
    }

    pub fn cfg_in_out_bundling(
        &mut self,
        program: &EthIRProgram,
    ) -> &ControlFlowGraphInOutBundling {
        if !self.cfg_in_out_bundling.valid {
            self.cfg_in_out_bundling.inner.compute(program);
            self.cfg_in_out_bundling.valid = true;
        }
        &self.cfg_in_out_bundling.inner
    }
}
