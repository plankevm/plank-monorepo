pub mod analyses;
pub mod optimizations;
pub mod transforms;

use optimizations::{
    constant_propagation::SCCPAnalysis, copy_propagation::CopyPropagation,
    unused_operation_elimination::UnusedOperationElimination,
};
use sir_data::EthIRProgram;
use transforms::ssa_transform::SsaTransform;

pub use analyses::{
    AnalysesMask, AnalysesStore, AnalysisKind, BasicBlockOwnershipAndReachability,
    ControlFlowGraphInOutBundling, DefUse, DominanceFrontiers, Dominators, InOutGroupId,
    Predecessors, UseKind, UseLocation, legalize,
};
pub use optimizations::{Defragmenter, OPTIMIZE_HELP, parse_optimizations_string};

pub trait Pass {
    fn run(&mut self, program: &mut EthIRProgram, store: &AnalysesStore);
    fn preserves(&self) -> AnalysesMask;
}

pub fn run_pass<T: Pass + Default>(
    pass: &mut Option<T>,
    program: &mut EthIRProgram,
    store: &AnalysesStore,
) {
    let pass = pass.get_or_insert_with(T::default);
    pass.run(program, store);
    store.invalidate_all_except(pass.preserves());
}

pub struct PassManager<'a> {
    program: &'a mut EthIRProgram,
    store: AnalysesStore,

    ssa_transform: Option<SsaTransform>,
    sccp: Option<SCCPAnalysis>,
    copy_prop: Option<CopyPropagation>,
    unused_elim: Option<UnusedOperationElimination>,
    defragmenter: Option<Defragmenter>,
}

impl<'a> PassManager<'a> {
    pub fn new(program: &'a mut EthIRProgram) -> Self {
        Self {
            program,
            store: AnalysesStore::default(),
            ssa_transform: None,
            sccp: None,
            copy_prop: None,
            unused_elim: None,
            defragmenter: None,
        }
    }

    pub fn run_legalize(&self) -> Result<(), analyses::LegalizerError> {
        legalize(self.program, &self.store)
    }

    pub fn run_ssa_transform(&mut self) {
        run_pass(&mut self.ssa_transform, self.program, &self.store);
        self.run_legalize().expect("IR is illegal after SSA transform");
    }

    pub fn run_optimizations(&mut self, passes: &str) {
        for c in passes.chars() {
            match c {
                's' => run_pass(&mut self.sccp, self.program, &self.store),
                'c' => run_pass(&mut self.copy_prop, self.program, &self.store),
                'u' => run_pass(&mut self.unused_elim, self.program, &self.store),
                'd' => run_pass(&mut self.defragmenter, self.program, &self.store),
                _ => unreachable!("should've been validated"),
            }
        }
        debug_assert!(legalize(self.program, &self.store).is_ok(), "optimized IR is illegal");
    }
}

#[cfg(test)]
pub(crate) fn run_pass_and_display(source: &str, pass: &mut impl Pass) -> String {
    let mut ir = sir_parser::parse_or_panic(source, sir_parser::EmitConfig::init_only());
    let store = AnalysesStore::default();
    pass.run(&mut ir, &store);
    sir_data::display_program(&ir)
}
