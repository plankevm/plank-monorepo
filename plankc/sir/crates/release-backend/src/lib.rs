use sir_assembler::Assembler;
use sir_data::EthIRProgram;
use sir_passes::{AnalysesStore, ControlFlowGraphInOutBundling};

use layouts::{LayoutsTracker, build_basic_block_layout_sets};
use stack::ScheduleConfig;

mod layouts;
mod op_graph;
mod op_model;
mod scheduler;
mod stack;

pub fn lower(
    program: &EthIRProgram,
    analyses: &AnalysesStore,
    asm: &mut Assembler,
    config: ScheduleConfig,
) {
    asm.clear();

    let in_out_bundling = ControlFlowGraphInOutBundling::new(program, analyses);
    let layout_sets = build_basic_block_layout_sets(program, analyses, &in_out_bundling);

    let layouts = LayoutsTracker::new(program, layout_sets, in_out_bundling);
}
