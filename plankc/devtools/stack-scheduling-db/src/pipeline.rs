use sir_data::BasicBlockId;
use sir_stack_scheduling::{
    ShuffleConfig,
    op_graph::{CanonicalizedBlock, OpGraph, build_graph_effectful, canonicalize_block_for_dedup},
    schedule,
    stack::StackOps,
};
use sir_stack_scheduling_common::prepare_program;
use std::path::Path;

pub fn run(
    source: &str,
    source_path: &Path,
    mut consume_block: impl FnMut(BasicBlockId, &OpGraph, &CanonicalizedBlock, &[StackOps]),
) {
    let prepared = prepare_program(source, source_path);
    let (scheduled, layouts, _next_alloc_id) =
        schedule(&prepared.program, &prepared.analyses, ShuffleConfig::PRE_AMSTERDAM);
    for (block_id, stack_ops) in scheduled.enumerate_idx() {
        let block = prepared.program.block(block_id);
        let (input_layout, output_layout) = layouts
            .get_input_output(block_id)
            .expect("scheduled block does not have input and output layouts");
        let graph = build_graph_effectful(
            &prepared.program,
            block,
            &layouts,
            input_layout,
            output_layout,
            &prepared.analyses,
        );
        let canonicalized = canonicalize_block_for_dedup(block, &graph);
        consume_block(block_id, &graph, &canonicalized, stack_ops);
    }
}
