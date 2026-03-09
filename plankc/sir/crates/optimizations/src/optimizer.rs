use sir_analyses::{AnalysesStore, AnalysisKind, Cached};
use sir_data::{BasicBlockId, DenseIndexSet, EthIRProgram};

use crate::{
    constant_propagation::SCCPAnalysis, copy_propagation::CopyPropagation,
    defragmenter::Defragmenter, unused_operation_elimination::UnusedOperationElimination,
};

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

pub(crate) fn run_optimization(
    opt: &mut impl Optimization,
    program: &mut EthIRProgram,
    store: &mut OptimizationStore,
) {
    opt.run(program, store);
    for kind in opt.invalidates() {
        store.analyses.invalidate(*kind);
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

pub struct Optimizer {
    src: EthIRProgram,
    store: OptimizationStore,

    sccp: Option<SCCPAnalysis>,
    copy_prop: Option<CopyPropagation>,
    unused_elim: UnusedOperationElimination,
    defragmenter: Option<Defragmenter>,
}

impl Optimizer {
    pub fn new(program: EthIRProgram) -> Self {
        Self {
            src: program,
            store: OptimizationStore::new(),
            sccp: None,
            copy_prop: None,
            unused_elim: UnusedOperationElimination::new(),
            defragmenter: None,
        }
    }

    pub fn run_passes(&mut self, passes: &str) {
        for c in passes.chars() {
            match c {
                's' => self.run_sccp(),
                'c' => self.run_copy_prop(),
                'u' => self.run_unused_elim(),
                'd' => self.run_defragment(),
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

    fn run_sccp(&mut self) {
        let sccp = self.sccp.get_or_insert_with(SCCPAnalysis::new);
        run_optimization(sccp, &mut self.src, &mut self.store);
    }

    fn run_copy_prop(&mut self) {
        let copy_prop = self.copy_prop.get_or_insert_with(CopyPropagation::new);
        run_optimization(copy_prop, &mut self.src, &mut self.store);
    }

    fn run_unused_elim(&mut self) {
        run_optimization(&mut self.unused_elim, &mut self.src, &mut self.store);
    }

    fn run_defragment(&mut self) {
        let defragmenter = self.defragmenter.get_or_insert_with(Defragmenter::new);
        run_optimization(defragmenter, &mut self.src, &mut self.store);
    }
}

#[cfg(test)]
pub(crate) fn run_pass(source: &str, opt: &mut impl Optimization) -> String {
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
