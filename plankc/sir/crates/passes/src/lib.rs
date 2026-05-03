pub mod analyses;
pub mod optimizations;
pub mod transforms;

use optimizations::{
    constant_propagation::SCCP, copy_propagation::CopyPropagation, switch_peephole::SwitchPeephole,
    unused_operation_elimination::UnusedOperationElimination,
};
use sir_data::EthIRProgram;

use crate::transforms::SSATransform;
pub use analyses::{
    AnalysesMask, AnalysesStore, BasicBlockOwnershipAndReachability, ControlFlowGraphInOutBundling,
    DefUse, DominanceFrontiers, Dominators, InOutGroupId, Legalizer, Predecessors, UseKind,
    UseLocation,
};
pub use optimizations::{Defragmenter, OPTIMIZE_HELP, parse_optimizations_string};

pub trait Pass {
    fn run(&mut self, program: &mut EthIRProgram, store: &AnalysesStore);

    fn preserves(&self) -> AnalysesMask {
        AnalysesMask::empty()
    }
}

pub fn run_pass<T: Pass>(pass: &mut T, program: &mut EthIRProgram, store: &AnalysesStore) {
    pass.run(program, store);
    store.invalidate_all_except(pass.preserves());
}

pub struct PassManager<'a> {
    program: &'a mut EthIRProgram,
    store: AnalysesStore,

    legalizer: Option<Legalizer>,
    sccp: Option<SCCP>,
    copy_prop: Option<CopyPropagation>,
    unused_elim: Option<UnusedOperationElimination>,
    defragmenter: Option<Defragmenter>,
    switch_peephole: Option<SwitchPeephole>,
}

impl<'a> PassManager<'a> {
    pub fn new(program: &'a mut EthIRProgram) -> Self {
        Self {
            program,
            store: AnalysesStore::default(),
            legalizer: None,
            sccp: None,
            copy_prop: None,
            unused_elim: None,
            defragmenter: None,
            switch_peephole: None,
        }
    }

    pub fn run_legalize(&mut self) -> Result<(), analyses::LegalizerError> {
        self.legalizer.get_or_insert_default().run(self.program, &self.store)
    }

    pub fn run_ssa_transform(&mut self) {
        run_pass(&mut SSATransform, self.program, &self.store);
        self.run_legalize().expect("IR is illegal after SSA transform");
    }

    pub fn run_optimizations(&mut self, passes: &str) {
        use optimizations::OptimizationPass;
        for c in passes.chars() {
            match OptimizationPass::from_char(c).expect("validated") {
                OptimizationPass::Sccp => {
                    run_pass(self.sccp.get_or_insert_default(), self.program, &self.store)
                }
                OptimizationPass::CopyPropagation => {
                    run_pass(self.copy_prop.get_or_insert_default(), self.program, &self.store)
                }
                OptimizationPass::UnusedElimination => {
                    run_pass(self.unused_elim.get_or_insert_default(), self.program, &self.store)
                }
                OptimizationPass::Defragment => {
                    run_pass(self.defragmenter.get_or_insert_default(), self.program, &self.store)
                }
                OptimizationPass::SwitchPeephole => run_pass(
                    self.switch_peephole.get_or_insert_default(),
                    self.program,
                    &self.store,
                ),
            }
        }
        debug_assert!(self.run_legalize().is_ok(), "optimized IR is illegal");
    }
}

#[cfg(test)]
pub(crate) fn run_pass_and_display<T: Pass + Default>(source: &str) -> EthIRProgram {
    let mut ir = sir_parser::parse_or_panic(source, sir_parser::EmitConfig::init_only());
    let store = AnalysesStore::default();
    run_pass(&mut T::default(), &mut ir, &store);
    ir
}
