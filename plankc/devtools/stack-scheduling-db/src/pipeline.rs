use crate::inline_constants::inline_constants_at_each_use;
use sir_data::BasicBlockId;
use sir_parser::{EmitConfig, parse_or_panic};
use sir_passes::{AnalysesStore, Legalizer, run_pass, transforms::CriticalEdgeSplitting};
use sir_stack_scheduling::{
    ShuffleConfig,
    op_graph::{CanonicalizedBlock, OpGraph, build_graph_effectful, canonicalize_block_for_dedup},
    schedule,
    stack::StackOps,
};
use std::path::Path;

pub fn run(
    source: &str,
    source_path: &Path,
    mut consume_block: impl FnMut(BasicBlockId, &OpGraph, &CanonicalizedBlock, &[StackOps]),
) {
    let mut program = parse_or_panic(source, EmitConfig::default());
    inline_constants_at_each_use(&mut program);

    let analyses = AnalysesStore::default();
    run_pass(&mut CriticalEdgeSplitting, &mut program, &analyses);
    Legalizer::default().run(&program, &analyses).unwrap_or_else(|error| {
        panic!("prepared SIR for '{}' is illegal: {error}", source_path.display())
    });

    let (scheduled, layouts, _next_alloc_id) =
        schedule(&program, &analyses, ShuffleConfig::PRE_AMSTERDAM);
    for (block_id, stack_ops) in scheduled.enumerate_idx() {
        let block = program.block(block_id);
        let (input_layout, output_layout) = layouts
            .get_input_output(block_id)
            .expect("scheduled block does not have input and output layouts");
        let graph = build_graph_effectful(
            &program,
            block,
            &layouts,
            input_layout,
            output_layout,
            &analyses,
        );
        let canonicalized = canonicalize_block_for_dedup(block, &graph);
        consume_block(block_id, &graph, &canonicalized, stack_ops);
    }
}
