use plank_core::list_of_lists::ListOfLists;
use sir_data::{BasicBlockId, EthIRProgram};
use sir_passes::{AnalysesStore, ControlFlowGraphInOutBundling};

use layouts::{LayoutsTracker, build_basic_block_layout_sets};
use op_graph::build_graph_simple;
use stack::ScheduleConfig;

use crate::{scheduler::dumb_schedule, stack::StackOps};

mod layouts;
mod op_graph;
mod op_model;
mod scheduler;
pub mod stack;

pub fn lower<'ir>(
    program: &'ir EthIRProgram,
    analyses: &AnalysesStore,
    config: ScheduleConfig,
) -> (ListOfLists<BasicBlockId, StackOps>, LayoutsTracker<'ir>) {
    let in_out_bundling = ControlFlowGraphInOutBundling::new(program, analyses);
    let layout_sets = build_basic_block_layout_sets(program, analyses, &in_out_bundling);

    // Naively take layout sets as layouts since they are deterministically ordered.
    let layouts = LayoutsTracker::new(program, layout_sets, in_out_bundling);

    let mut stack_ops = ListOfLists::new();
    for block in program.blocks() {
        let Some((input_layout, output_layout)) = layouts.get_input_output(block.id()) else {
            assert_eq!(stack_ops.push_copy_slice(&[]), block.id());
            continue;
        };
        let graph = build_graph_simple(program, block, &layouts, input_layout, output_layout);
        let bb_id = stack_ops.push_with(|mut pusher| {
            dumb_schedule(&mut pusher, block, &program.next_static_alloc_id, config, &graph);
        });
        assert_eq!(bb_id, block.id());
    }

    (stack_ops, layouts)
}

#[cfg(test)]
mod tests;
