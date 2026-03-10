use sir_analyses::{AnalysesStore, AnalysisKind, Cached};
use sir_data::{BasicBlockId, DenseIndexSet, EthIRProgram};

use crate::{
    constant_propagation::SCCPAnalysis, copy_propagation::CopyPropagation,
    defragmenter::Defragmenter, unused_operation_elimination::UnusedOperationElimination,
};

pub struct Optimizer {
    src: EthIRProgram,
    store: OptimizationStore,

    sccp: Option<SCCPAnalysis>,
    copy_prop: Option<CopyPropagation>,
    unused_elim: Option<UnusedOperationElimination>,
    defragmenter: Option<Defragmenter>,
}

impl Optimizer {
    pub fn new(program: EthIRProgram) -> Self {
        Self {
            src: program,
            store: OptimizationStore::new(),
            sccp: None,
            copy_prop: None,
            unused_elim: None,
            defragmenter: None,
        }
    }

    pub fn run_passes(&mut self, passes: &str) {
        for c in passes.chars() {
            match c {
                's' => run_optimization(&mut self.sccp, &mut self.src, &mut self.store),
                'c' => run_optimization(&mut self.copy_prop, &mut self.src, &mut self.store),
                'u' => run_optimization(&mut self.unused_elim, &mut self.src, &mut self.store),
                'd' => run_optimization(&mut self.defragmenter, &mut self.src, &mut self.store),
                _ => unreachable!("should've been validated"),
            }
        }
    }

    pub fn finish(mut self) -> EthIRProgram {
        debug_assert!(
            sir_analyses::legalize(&self.src, &mut self.store.analyses).is_ok(),
            "optimized IR is illegal"
        );
        self.src
    }
}

pub(crate) fn run_optimization<T: Optimization + Default>(
    optimization: &mut Option<T>,
    program: &mut EthIRProgram,
    store: &mut OptimizationStore,
) {
    let optimization = optimization.get_or_insert_with(T::default);
    optimization.run(program, store);
    for kind in optimization.invalidates() {
        store.analyses.invalidate(*kind);
    }
}

pub trait Optimization {
    fn run(&mut self, program: &mut EthIRProgram, store: &mut OptimizationStore);
    fn invalidates(&self) -> &[AnalysisKind];
}

pub struct OptimizationStore {
    pub analyses: AnalysesStore,
    pub sccp_reachable: Cached<DenseIndexSet<BasicBlockId>>,
}

impl OptimizationStore {
    pub fn new() -> Self {
        Self {
            analyses: AnalysesStore::default(),
            sccp_reachable: Cached::new(DenseIndexSet::new()),
        }
    }

    pub fn sccp_reachable(&self) -> Option<&DenseIndexSet<BasicBlockId>> {
        self.sccp_reachable.get_if_valid()
    }
}

pub fn parse_passes_string(s: &str) -> Result<String, String> {
    for c in s.chars() {
        if !matches!(c, 's' | 'c' | 'u' | 'd') {
            return Err(format!(
                "invalid optimization pass '{}', valid passes: s (SCCP), c (copy propagation), u (unused elimination), d (defragment)",
                c
            ));
        }
    }
    Ok(s.to_string())
}

#[cfg(test)]
pub(crate) fn run_pass_and_display(source: &str, opt: &mut impl Optimization) -> String {
    let mut ir = sir_parser::parse_or_panic(source, sir_parser::EmitConfig::init_only());
    let mut store = OptimizationStore::new();
    opt.run(&mut ir, &mut store);
    sir_data::display_program(&ir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sir_parser::{EmitConfig, parse_or_panic};
    use sir_test_utils::assert_trim_strings_eq_with_diff;

    fn optimize(source: &str, passes: &str) -> String {
        let program = parse_or_panic(source, EmitConfig::init_only());
        let mut optimizer = Optimizer::new(program);
        optimizer.run_passes(passes);
        let program = optimizer.finish();
        sir_data::display_program(&program)
    }

    const SWITCH_ON_COPY_WITH_DEAD_CODE: &str = r#"
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

    #[test]
    fn test_csud() {
        let expected = r#"
Init: @0
Functions:
    fn @0 -> entry @0  (outputs: 0)

Basic Blocks:
    @0 {
        => @1
    }

    @1 {
        stop
    }
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "csud");
        assert_trim_strings_eq_with_diff(&actual, expected, "csud");
    }

    #[test]
    fn test_cusd() {
        let expected = r#"
Init: @0
Functions:
    fn @0 -> entry @0  (outputs: 0)

Basic Blocks:
    @0 {
        $0 = const 0x1
        => @1
    }

    @1 {
        stop
    }
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "cusd");
        assert_trim_strings_eq_with_diff(&actual, expected, "cusd");
    }

    #[test]
    fn test_ucsd() {
        let expected = r#"
Init: @0
Functions:
    fn @0 -> entry @0  (outputs: 0)

Basic Blocks:
    @0 {
        $0 = const 0x1
        $1 = copy $0
        => @1
    }

    @1 {
        stop
    }
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "ucsd");
        assert_trim_strings_eq_with_diff(&actual, expected, "ucsd");
    }

    #[test]
    fn test_uscd() {
        let expected = r#"
Init: @0
Functions:
    fn @0 -> entry @0  (outputs: 0)

Basic Blocks:
    @0 {
        $0 = const 0x1
        $1 = copy $0
        switch $0 {
            1 => @1,
            else => @2
        }

    }

    @1 {
        stop
    }

    @2 {
        $2 = const 0x0
        => @1
    }
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "uscd");
        assert_trim_strings_eq_with_diff(&actual, expected, "uscd");
    }

    #[test]
    fn test_store_invalidation_and_recomputation() {
        use crate::{
            constant_propagation::SCCPAnalysis, copy_propagation::CopyPropagation,
            defragmenter::Defragmenter, unused_operation_elimination::UnusedOperationElimination,
        };
        use sir_analyses::AnalysisKind;

        let mut program = parse_or_panic(SWITCH_ON_COPY_WITH_DEAD_CODE, EmitConfig::init_only());
        let mut store = OptimizationStore::new();

        // Computing dominance_frontiers transitively computes predecessors and dominators
        store.analyses.dominance_frontiers(&program);
        assert!(store.analyses.is_valid(AnalysisKind::Predecessors));
        assert!(store.analyses.is_valid(AnalysisKind::Dominators));
        assert!(store.analyses.is_valid(AnalysisKind::DominanceFrontiers));

        // SCCP invalidates DefUse, Predecessors (cascades to Dominators, DominanceFrontiers),
        // BasicBlockOwnership, CfgInOutBundling — and populates sccp_reachable
        let mut sccp: Option<SCCPAnalysis> = None;
        run_optimization(&mut sccp, &mut program, &mut store);
        assert!(!store.analyses.is_valid(AnalysisKind::DefUse));
        assert!(!store.analyses.is_valid(AnalysisKind::Predecessors));
        assert!(!store.analyses.is_valid(AnalysisKind::Dominators));
        assert!(!store.analyses.is_valid(AnalysisKind::DominanceFrontiers));
        assert!(!store.analyses.is_valid(AnalysisKind::BasicBlockOwnership));
        assert!(!store.analyses.is_valid(AnalysisKind::CfgInOutBundling));
        assert!(store.sccp_reachable.is_valid());

        // Defragmenter consumes sccp_reachable and invalidates it
        let mut defrag: Option<Defragmenter> = None;
        run_optimization(&mut defrag, &mut program, &mut store);
        assert!(!store.sccp_reachable.is_valid());

        // Copy prop invalidates DefUse
        let mut copy_prop: Option<CopyPropagation> = None;
        run_optimization(&mut copy_prop, &mut program, &mut store);
        assert!(!store.analyses.is_valid(AnalysisKind::DefUse));

        // def_use recomputes lazily and marks valid
        store.analyses.def_use(&program);
        assert!(store.analyses.is_valid(AnalysisKind::DefUse));

        // Unused elim uses def_use_mut: computes DefUse then marks it invalid
        let mut unused_elim: Option<UnusedOperationElimination> = None;
        run_optimization(&mut unused_elim, &mut program, &mut store);
        assert!(!store.analyses.is_valid(AnalysisKind::DefUse));

        // Defragmenter works without sccp_reachable
        assert!(!store.sccp_reachable.is_valid());
        run_optimization(&mut defrag, &mut program, &mut store);
    }

    #[test]
    fn test_scsud() {
        let expected = r#"
Init: @0
Functions:
    fn @0 -> entry @0  (outputs: 0)

Basic Blocks:
    @0 {
        => @1
    }

    @1 {
        stop
    }
        "#;

        let actual = optimize(SWITCH_ON_COPY_WITH_DEAD_CODE, "scsud");
        assert_trim_strings_eq_with_diff(&actual, expected, "scsud");
    }
}
