use crate::analyses::{function_effects::FunctionEffects, *};
use sir_data::EthIRProgram;
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

    fn get_mut(
        &self,
        program: &EthIRProgram,
        store: &AnalysesStore,
        compute: bool,
    ) -> RefMut<'_, T> {
        let mut cached = self.state.borrow_mut();
        if compute && !cached.valid {
            cached.analysis.compute(program, store);
        }
        cached.valid = false;
        RefMut::map(cached, |s| &mut s.analysis)
    }
}

impl<T> Cached<T> {
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

            $(
                pub fn $field(&self, program: &EthIRProgram) -> Ref<'_, $ty> {
                    self.$field.get(program, self)

                }
            )*
        }
    };
}

define_analyses! {
    DefUse => def_use: DefUse,
    Predecessors => predecessors: Predecessors,
    Dominators => dominators: Dominators,
    DominanceFrontiers => dominance_frontiers: DominanceFrontiers,
    BasicBlockOwnership => basic_block_ownership: BasicBlockOwnershipAndReachability,
    AllocationLiveness => allocation_liveness: AllocationLiveness,
    LocalLiveness => local_liveness: LocalLiveness,
    ReachableBlocks => reachable_blocks: ReachableBlocks,
    ReversePostOrder => reverse_post_order: ReversePostOrder,
    FunctionEffects => function_effects: FunctionEffects,
}

impl AnalysesStore {
    pub fn def_use_mut(&self, program: &EthIRProgram) -> RefMut<'_, DefUse> {
        self.def_use.get_mut(program, self, true)
    }

    pub fn reachable_blocks_mut(
        &self,
        program: &EthIRProgram,
        compute: bool,
    ) -> RefMut<'_, ReachableBlocks> {
        self.reachable_blocks.get_mut(program, self, compute)
    }

    pub fn predecessors_mut(&self, program: &EthIRProgram) -> RefMut<'_, Predecessors> {
        self.predecessors.get_mut(program, self, true)
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
        transforms::SSATransform,
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
        // BasicBlockOwnership, CfgInOutBundling — and populates reachable_blocks
        run_pass(&mut SCCP::default(), &mut program, &store);
        assert!(!store.def_use.is_valid());
        assert!(!store.predecessors.is_valid());
        assert!(!store.dominators.is_valid());
        assert!(!store.dominance_frontiers.is_valid());
        assert!(!store.basic_block_ownership.is_valid());
        assert!(store.reachable_blocks.is_valid());

        // Defragmenter consumes reachable_blocks analysis and invalidates it
        let mut defrag = Defragmenter::default();
        run_pass(&mut defrag, &mut program, &store);
        assert!(!store.reachable_blocks.is_valid());

        // Copy prop invalidates DefUse
        run_pass(&mut CopyPropagation::default(), &mut program, &store);
        assert!(!store.def_use.is_valid());

        // def_use recomputes lazily and marks valid
        store.def_use(&program);
        assert!(store.def_use.is_valid());

        // Unused elim uses def_use_mut: computes DefUse then marks it invalid
        run_pass(&mut UnusedOperationElimination::default(), &mut program, &store);
        assert!(!store.def_use.is_valid());
    }

    #[test]
    fn test_ssa_transform_preserves_recomputed_analyses() {
        let mut program = parse_or_panic(
            r#"
                fn init:
                    entry {
                        cond = const 1
                        => cond ? @entry : @exit
                    }
                    exit { stop }
                    orphan { stop }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        store.predecessors(&program);
        store.reverse_post_order(&program);

        run_pass(&mut SSATransform, &mut program, &store);

        assert!(store.predecessors.is_valid());
        assert!(store.reachable_blocks.is_valid());
        assert!(store.reverse_post_order.is_valid());

        let snapshot = |store: &AnalysesStore| {
            let predecessors = store.predecessors(&program);
            let reachable_blocks = store.reachable_blocks(&program);
            let reverse_post_order = store.reverse_post_order(&program);
            (
                predecessors
                    .enumerate()
                    .map(|(block, predecessors)| (block, predecessors.to_vec()))
                    .collect::<Vec<_>>(),
                program
                    .basic_blocks
                    .iter_idx()
                    .filter(|&block| reachable_blocks.contains(block))
                    .collect::<Vec<_>>(),
                reverse_post_order.blocks_rpo().copied().collect::<Vec<_>>(),
            )
        };
        let recomputed_store = AnalysesStore::default();
        assert_eq!(snapshot(&store), snapshot(&recomputed_store));
    }
}
