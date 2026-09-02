use crate::inline_constants::inline_constants_at_each_use;
use sir_data::EthIRProgram;
use sir_parser::{EmitConfig, parse_or_panic};
use sir_passes::{AnalysesStore, Legalizer, run_pass, transforms::CriticalEdgeSplitting};
use sir_stack_scheduling::{ScheduledOps, ShuffleConfig, schedule};
use std::path::Path;

pub struct PipelineOutput {
    pub program: EthIRProgram,
    pub scheduled: ScheduledOps,
}

#[derive(Default)]
pub struct StackSchedulingPipeline;

impl StackSchedulingPipeline {
    pub fn run(source: &str, source_path: &Path) -> PipelineOutput {
        let mut program = parse_or_panic(source, EmitConfig::default());
        inline_constants_at_each_use(&mut program);

        let analyses = AnalysesStore::default();
        run_pass(&mut CriticalEdgeSplitting, &mut program, &analyses);
        Legalizer::default().run(&program, &analyses).unwrap_or_else(|error| {
            panic!("prepared SIR for '{}' is illegal: {error}", source_path.display())
        });

        let (scheduled, _layouts, _next_alloc_id) =
            schedule(&program, &analyses, ShuffleConfig::PRE_AMSTERDAM);
        PipelineOutput { program, scheduled }
    }
}
