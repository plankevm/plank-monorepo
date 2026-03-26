use crate::analyses::{
    AllocationLiveness, BasicBlockOwnershipAndReachability, ControlFlowGraphInOutBundling, DefUse,
    DominanceFrontiers, Dominators, LocalLiveness, Predecessors,
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

    pub(crate) fn get_mut_maybe_invalid(&self) -> RefMut<'_, T> {
        RefMut::map(self.state.borrow_mut(), |s| &mut s.analysis)
    }

    pub(crate) fn mark_valid(&self) {
        self.state.borrow_mut().valid = true;
    }

    fn is_valid(&self) -> bool {
        self.state.borrow().valid
    }

    fn invalidate(&self) {
        self.state.borrow_mut().valid = false;
    }
}

macro_rules! define_analyses {
    ($($variant:ident => $field:ident : $ty:ty),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum AnalysisKind {
            $($variant),*
        }

        bitflags::bitflags! {
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
            pub struct AnalysesMask: u32 {
                $(const $variant = 1 << (AnalysisKind::$variant as u8);)*
            }
        }

        #[derive(Default)]
        pub struct AnalysesStore {
            $(pub(crate) $field: Cached<$ty>),*
        }

        impl AnalysesStore {
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
    AllocationLiveness => allocation_liveness: AllocationLiveness,
    LocalLiveness => local_liveness: LocalLiveness,
    // Produced by SCCP, not computed on-demand. Use get_buffer() + mark_valid().
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

    pub fn allocation_liveness(&self, program: &EthIRProgram) -> Ref<'_, AllocationLiveness> {
        self.allocation_liveness.get(program, self)
    }

    pub fn local_liveness(&self, program: &EthIRProgram) -> Ref<'_, LocalLiveness> {
        self.local_liveness.get(program, self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        optimizations::{
            constant_propagation::SCCP, copy_propagation::CopyPropagation,
            defragmenter::Defragmenter, unused_operation_elimination::UnusedOperationElimination,
        },
        run_pass,
    };
    use sir_parser::{EmitConfig, parse_or_panic};

    #[test]
    fn test_store_invalidation_and_recomputation() {
        let source = r#"
            fn init:
                entry {
                    x = const 1
                    y = copy x
                    switch y {
                        1 => @one
                        default => @other
                    }
                }
                one {
                    dead = const 42
                    stop
                }
                other {
                    cond = const 0
                    => cond ? @other_yes : @one
                }
                other_yes { stop }
        "#;

        let mut program = parse_or_panic(source, EmitConfig::init_only());
        let store = AnalysesStore::default();

        // Computing dominance_frontiers transitively computes predecessors and dominators
        store.dominance_frontiers(&program);
        assert!(store.predecessors.is_valid());
        assert!(store.dominators.is_valid());
        assert!(store.dominance_frontiers.is_valid());

        // SCCP invalidates DefUse, Predecessors (cascades to Dominators, DominanceFrontiers),
        // BasicBlockOwnership, CfgInOutBundling — and populates sccp_reachable
        run_pass(&mut SCCP::default(), &mut program, &store);
        assert!(!store.def_use.is_valid());
        assert!(!store.predecessors.is_valid());
        assert!(!store.dominators.is_valid());
        assert!(!store.dominance_frontiers.is_valid());
        assert!(!store.basic_block_ownership.is_valid());
        assert!(!store.cfg_in_out_bundling.is_valid());
        assert!(store.sccp_reachable.is_valid());

        // Defragmenter consumes sccp_reachable and invalidates it
        let mut defrag = Defragmenter::default();
        run_pass(&mut defrag, &mut program, &store);
        assert!(!store.sccp_reachable.is_valid());

        // Copy prop invalidates DefUse
        run_pass(&mut CopyPropagation::default(), &mut program, &store);
        assert!(!store.def_use.is_valid());

        // def_use recomputes lazily and marks valid
        store.def_use(&program);
        assert!(store.def_use.is_valid());

        // Unused elim uses def_use_mut: computes DefUse then marks it invalid
        run_pass(&mut UnusedOperationElimination::default(), &mut program, &store);
        assert!(!store.def_use.is_valid());

        // Defragmenter works without sccp_reachable
        assert!(!store.sccp_reachable.is_valid());
        run_pass(&mut defrag, &mut program, &store);
    }
}
