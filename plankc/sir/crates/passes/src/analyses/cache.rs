use crate::{
    BasicBlockOwnershipAndReachability, ControlFlowGraphInOutBundling, DefUse, DominanceFrontiers,
    Dominators, Predecessors,
};
use sir_data::{BasicBlockId, DenseIndexSet, EthIRProgram};
use std::cell::{Ref, RefCell, RefMut};

#[derive(Default)]
pub(crate) struct Cached<T> {
    state: RefCell<CachedState<T>>,
}

#[derive(Default)]
pub(crate) struct CachedState<T> {
    pub(crate) analysis: T,
    pub(crate) valid: bool,
}

pub(crate) trait Analysis {
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore);
}

impl<T: Analysis> Cached<T> {
    fn get(&self, program: &EthIRProgram, store: &AnalysesStore) -> Ref<'_, T> {
        if !self.is_valid() {
            let mut cached = self.state.borrow_mut();
            cached.analysis.compute(program, store);
            cached.valid = true;
        }
        Ref::map(self.state.borrow(), |s| &s.analysis)
    }

    fn get_mut(&self, program: &EthIRProgram, store: &AnalysesStore) -> RefMut<'_, T> {
        let mut cached = self.state.borrow_mut();
        if !cached.valid {
            cached.analysis.compute(program, store);
        }
        cached.valid = false;
        RefMut::map(cached, |s| &mut s.analysis)
    }
}

impl<T> Cached<T> {
    pub(crate) fn get_if_valid(&self) -> Option<Ref<'_, T>> {
        if self.is_valid() { Some(Ref::map(self.state.borrow(), |s| &s.analysis)) } else { None }
    }

    pub(crate) fn get_buffer(&self) -> RefMut<'_, T> {
        RefMut::map(self.state.borrow_mut(), |s| &mut s.analysis)
    }

    pub(crate) fn mark_valid(&self) {
        self.state.borrow_mut().valid = true;
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.state.borrow().valid
    }

    pub(crate) fn invalidate(&self) {
        self.state.borrow_mut().valid = false;
    }
}

macro_rules! define_analyses {
    ($($variant:ident => $field:ident : $ty:ty),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum AnalysisKind {
            $($variant),*
        }

        impl AnalysisKind {
            pub const ALL: &[AnalysisKind] = &[$(AnalysisKind::$variant),*];
        }

        bitflags::bitflags! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
            pub struct AnalysesMask: u8 {
                $(const $variant = 1 << (AnalysisKind::$variant as u8);)*
            }
        }

        #[derive(Default)]
        pub struct AnalysesStore {
            $(pub(crate) $field: Cached<$ty>),*
        }

        impl AnalysesStore {
            pub fn is_valid(&self, kind: AnalysisKind) -> bool {
                match kind {
                    $(AnalysisKind::$variant => self.$field.is_valid()),*
                }
            }

            pub fn invalidate_all_except(&self, preserved: AnalysesMask) {
                $(if !preserved.contains(AnalysesMask::$variant) {
                    self.$field.invalidate();
                })*
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
    SccpReachable => sccp_reachable: DenseIndexSet<BasicBlockId>,
}

impl AnalysesStore {
    pub fn def_use(&self, program: &EthIRProgram) -> Ref<'_, DefUse> {
        self.def_use.get(program, self)
    }

    pub fn def_use_mut(&self, program: &EthIRProgram) -> RefMut<'_, DefUse> {
        self.def_use.get_mut(program, self)
    }

    pub fn predecessors(&self, program: &EthIRProgram) -> Ref<'_, Predecessors> {
        self.predecessors.get(program, self)
    }

    pub fn dominators(&self, program: &EthIRProgram) -> Ref<'_, Dominators> {
        self.dominators.get(program, self)
    }

    pub fn dominance_frontiers(&self, program: &EthIRProgram) -> Ref<'_, DominanceFrontiers> {
        self.dominance_frontiers.get(program, self)
    }

    pub fn basic_block_ownership(
        &self,
        program: &EthIRProgram,
    ) -> Ref<'_, BasicBlockOwnershipAndReachability> {
        self.basic_block_ownership.get(program, self)
    }

    pub fn cfg_in_out_bundling(
        &self,
        program: &EthIRProgram,
    ) -> Ref<'_, ControlFlowGraphInOutBundling> {
        self.cfg_in_out_bundling.get(program, self)
    }
}
