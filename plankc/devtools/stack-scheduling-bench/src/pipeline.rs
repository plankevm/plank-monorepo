use sir_data::EthIRProgram;
use sir_stack_scheduling::{ScheduledOps, ShuffleConfig, schedule};
use sir_stack_scheduling_common::prepare_program;
use std::path::Path;

pub struct PipelineOutput {
    pub program: EthIRProgram,
    pub scheduled: ScheduledOps,
}

#[derive(Default)]
pub struct StackSchedulingPipeline;

impl StackSchedulingPipeline {
    pub fn run(source: &str, source_path: &Path) -> PipelineOutput {
        let prepared = prepare_program(source, source_path);
        let (scheduled, _layouts, _next_alloc_id) =
            schedule(&prepared.program, &prepared.analyses, ShuffleConfig::PRE_AMSTERDAM);
        PipelineOutput { program: prepared.program, scheduled }
    }
}
