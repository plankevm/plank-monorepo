use crate::inline_constants::inline_constants_at_each_use;
use sir_data::EthIRProgram;
use sir_parser::{EmitConfig, parse_or_panic};
use sir_passes::{AnalysesStore, Legalizer, run_pass, transforms::CriticalEdgeSplitting};
use std::path::Path;

pub struct PreparedProgram {
    pub program: EthIRProgram,
    pub analyses: AnalysesStore,
}

pub fn prepare_program(source: &str, source_path: &Path) -> PreparedProgram {
    let mut program = parse_or_panic(source, EmitConfig::default());
    inline_constants_at_each_use(&mut program);

    let analyses = AnalysesStore::default();
    run_pass(&mut CriticalEdgeSplitting, &mut program, &analyses);
    Legalizer::default().run(&program, &analyses).unwrap_or_else(|error| {
        panic!("prepared SIR for '{}' is illegal: {error}", source_path.display())
    });

    PreparedProgram { program, analyses }
}
