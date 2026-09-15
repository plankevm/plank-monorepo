use std::{collections::BTreeMap, num::NonZero};

use plank_core::{DenseIndexMap, list_of_lists::ListOfLists, newtype_index};
use rayon::prelude::*;
use sir_data::{BasicBlockId, EthIRProgram, Operation, StaticAllocId};
use sir_passes::{AnalysesStore, ControlFlowGraphInOutBundling};

use layouts::{
    GlobalSpills, LayoutsTracker, MemoryReturnDestinations, Rematerialization, Rematerializations,
    build_basic_block_layout_sets, order_layouts, select_call_rematerializations,
    select_global_spills, select_memory_call_arguments,
};
pub use stack::ShuffleConfig;
pub mod op_graph;

use crate::{
    op_graph::{build_graph_effectful_with_spills, tail_call_in_block},
    stack::StackOps,
};

mod depth_first_search;
mod greedy_intra_op_scheduler;
mod greedy_shuffler;
mod layouts;
mod scheduler;
pub mod stack;
pub mod treegraph;

newtype_index! {
    pub struct StackOpIdx;
}

const AVG_OPS_PER_BLOCK: usize = 20;
const DEFAULT_MAX_SEARCH_CANDIDATES: usize = 1_000;
const BLOCK_SCHEDULING_THREADS: usize = 6;

fn estimated_stack_management_cost(
    ops: &[StackOps],
    shuffle_config: ShuffleConfig,
) -> (usize, u32) {
    ops.iter().fold((0, 0), |(bytes, gas), op| {
        let (op_bytes, op_gas) = match op {
            StackOps::Swap(_) | StackOps::Dup(_) | StackOps::Pop => (1, 3),
            StackOps::Exchange(0, _) | StackOps::Exchange(_, 0) => {
                (1, u32::from(shuffle_config.exchange_cost))
            }
            StackOps::Exchange(_, _) => (3, u32::from(shuffle_config.exchange_cost)),
            // Assume a PUSH1 address and exclude context-dependent memory expansion.
            StackOps::Store(_) | StackOps::Load(_) => (3, 6),
            // Necessary SIR operations are common to both candidate schedules.
            StackOps::Flipped(_)
            | StackOps::Op(_)
            | StackOps::MemoryReturnCall(_, _)
            | StackOps::TailCall(_)
            | StackOps::CallRetPush(_) => (0, 0),
        };
        (bytes + op_bytes, gas + op_gas)
    })
}

fn rematerialized_schedule_is_better(
    baseline: &[StackOps],
    rematerialized: &[StackOps],
    replay_bytecode_size: usize,
    replay_execution_gas: u32,
    shuffle_config: ShuffleConfig,
) -> bool {
    let (baseline_bytes, baseline_gas) = estimated_stack_management_cost(baseline, shuffle_config);
    let (rematerialized_bytes, rematerialized_gas) =
        estimated_stack_management_cost(rematerialized, shuffle_config);
    let rematerialized_bytes = rematerialized_bytes + replay_bytecode_size;
    let rematerialized_gas = rematerialized_gas + replay_execution_gas;
    rematerialized_bytes <= baseline_bytes
        && rematerialized_gas <= baseline_gas
        && (rematerialized_bytes < baseline_bytes || rematerialized_gas < baseline_gas)
}

fn rematerialize_global_loads(
    mut schedule: depth_first_search::SearchResult,
    constants: &BTreeMap<StaticAllocId, Rematerialization>,
    shuffle_config: ShuffleConfig,
) -> depth_first_search::SearchResult {
    if constants.is_empty() {
        return schedule;
    }
    let mut candidate = schedule.ops.to_vec();
    let mut replay_bytecode_size = 0;
    let mut replay_execution_gas = 0;
    for operation in &mut candidate {
        let StackOps::Load(slot) = operation else { continue };
        let Some(constant) = constants.get(slot) else { continue };
        // A load can be as small as PUSH0 + MLOAD, so larger pushes cannot guarantee a bytecode
        // non-regression until exact memory addresses are available.
        if constant.bytecode_size > 2 {
            continue;
        }
        *operation = StackOps::Op(constant.operation);
        replay_bytecode_size += usize::from(constant.bytecode_size);
        replay_execution_gas += if constant.bytecode_size == 1 { 2 } else { 3 };
    }
    if replay_bytecode_size != 0
        && rematerialized_schedule_is_better(
            &schedule.ops,
            &candidate,
            replay_bytecode_size,
            replay_execution_gas,
            shuffle_config,
        )
    {
        schedule.ops = candidate.into_boxed_slice();
    }
    schedule
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum LayoutOrdering {
    #[default]
    Naive,
    SuccessorOnly,
    SuccessorAndProducer,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CallArgumentStrategy {
    #[default]
    StackOnly,
    FullReliefOnly,
    PartialRelief,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GlobalSchedulerConfig {
    pub layout_ordering: LayoutOrdering,
    pub spill_dormant_values: bool,
    pub call_arguments: CallArgumentStrategy,
    pub rematerialize_small_constants: bool,
    pub eliminate_tail_calls: bool,
    pub memory_back_return_destinations: bool,
}

#[derive(Debug)]
pub struct ScheduledOps {
    bb_to_ops: DenseIndexMap<BasicBlockId, StackOpIdx>,
    ops: ListOfLists<StackOpIdx, StackOps>,
}

impl ScheduledOps {
    pub fn get(&self, bb: BasicBlockId) -> Option<&[StackOps]> {
        self.bb_to_ops.get(bb).map(|&idx| &self.ops[idx])
    }

    pub fn enumerate_idx(&self) -> impl Iterator<Item = (BasicBlockId, &[StackOps])> {
        self.bb_to_ops.iter().map(|(bb_id, &idx)| (bb_id, &self.ops[idx]))
    }
}

pub fn schedule<'ir>(
    program: &'ir EthIRProgram,
    analyses: &AnalysesStore,
    shuffle_config: ShuffleConfig,
) -> (ScheduledOps, LayoutsTracker<'ir>, StaticAllocId) {
    schedule_with_config(program, analyses, shuffle_config, GlobalSchedulerConfig::default())
}

pub fn schedule_with_config<'ir>(
    program: &'ir EthIRProgram,
    analyses: &AnalysesStore,
    shuffle_config: ShuffleConfig,
    global_config: GlobalSchedulerConfig,
) -> (ScheduledOps, LayoutsTracker<'ir>, StaticAllocId) {
    let in_out_bundling = ControlFlowGraphInOutBundling::new(program, analyses);
    let mut layout_sets = build_basic_block_layout_sets(program, analyses, &in_out_bundling);
    order_layouts(
        program,
        analyses,
        &in_out_bundling,
        &mut layout_sets,
        global_config.layout_ordering,
    );
    let mut global_spills = if global_config.spill_dormant_values {
        let mut spills = select_global_spills(
            program,
            analyses,
            &in_out_bundling,
            &layout_sets,
            usize::from(shuffle_config.max_swap_depth),
        );
        spills.persist_across_regions(program, &in_out_bundling, &layout_sets);
        spills
    } else {
        GlobalSpills::default()
    };
    global_spills.extend_call_arguments(select_memory_call_arguments(
        program,
        analyses,
        &in_out_bundling,
        &layout_sets,
        usize::from(shuffle_config.max_swap_depth),
        global_config.call_arguments,
    ));
    let global_spill_base = program.next_static_alloc_id;
    let spilled_constants = if global_config.rematerialize_small_constants {
        global_spills.rematerializable_constants(program, global_spill_base)
    } else {
        BTreeMap::new()
    };
    global_spills.remove_from_layouts(program, &in_out_bundling, &mut layout_sets);
    let return_destination_base =
        global_spill_base + u32::try_from(global_spills.len()).expect("too many global spills");
    let memory_return_destinations = MemoryReturnDestinations::select(
        program,
        analyses,
        &in_out_bundling,
        &mut layout_sets,
        usize::from(shuffle_config.max_swap_depth),
        return_destination_base,
        global_config.memory_back_return_destinations,
    );
    let rematerializations = if global_config.rematerialize_small_constants {
        select_call_rematerializations(
            program,
            analyses,
            &in_out_bundling,
            &layout_sets,
            usize::from(shuffle_config.max_swap_depth),
        )
    } else {
        Rematerializations::default()
    };
    let local_alloc_start = return_destination_base
        + u32::try_from(memory_return_destinations.len())
            .expect("too many memory return destinations");
    let mut next_alloc_id = local_alloc_start;

    // Freeze the selected layout sets as concrete layouts.
    let layouts = LayoutsTracker::new(program, layout_sets, in_out_bundling);

    let mut bb_to_ops = DenseIndexMap::with_capacity(program.basic_blocks.len());
    let mut ops = ListOfLists::with_capacities(
        program.basic_blocks.len(),
        program.basic_blocks.len() * AVG_OPS_PER_BLOCK,
    );

    let block_graphs = program
        .blocks()
        .filter_map(|block| {
            let (input_layout, output_layout) = layouts.get_input_output(block.id())?;
            let tail_call = global_config
                .eliminate_tail_calls
                .then(|| tail_call_in_block(program, block))
                .flatten()
                .filter(|&operation| {
                    let Operation::InternalCall(call) = program.operations[operation] else {
                        unreachable!()
                    };
                    memory_return_destinations.slot_for_block(block.id()).is_none()
                        && memory_return_destinations.slot_for_function(call.function).is_none()
                });
            let graph = build_graph_effectful_with_spills(
                program,
                block,
                &layouts,
                input_layout,
                output_layout,
                analyses,
                &global_spills,
                global_spill_base,
                &rematerializations,
                tail_call,
                &memory_return_destinations,
            );
            let (replay_bytecode_size, replay_execution_gas) = block
                .operations()
                .flat_map(|operation| rematerializations.for_call(operation.id()))
                .fold((0, 0), |(bytes, gas), rematerialization| {
                    (
                        bytes + usize::from(rematerialization.bytecode_size),
                        gas + if rematerialization.bytecode_size == 1 { 2 } else { 3 },
                    )
                });
            // Call pressure is only a candidate heuristic. Compare complete block schedules before
            // accepting the replay.
            let baseline = (replay_bytecode_size != 0).then(|| {
                (
                    build_graph_effectful_with_spills(
                        program,
                        block,
                        &layouts,
                        input_layout,
                        output_layout,
                        analyses,
                        &global_spills,
                        global_spill_base,
                        &Rematerializations::default(),
                        tail_call,
                        &memory_return_destinations,
                    ),
                    replay_bytecode_size,
                    replay_execution_gas,
                )
            });
            let return_destination =
                if matches!(block.control(), sir_data::ControlView::InternalReturn) {
                    memory_return_destinations.slot_for_block(block.id())
                } else {
                    None
                };
            Some((block, graph, baseline, return_destination, tail_call))
        })
        .collect::<Vec<_>>();
    // Blocks share a temporary spill base while scheduling so they can run independently. Their
    // block-local spill IDs are rebased after the parallel search finishes.
    let spill_alloc_start = global_spill_base;
    let scheduling_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(BLOCK_SCHEDULING_THREADS)
        .build()
        .expect("failed to create block scheduling thread pool");
    let block_schedules = scheduling_pool.install(|| {
        block_graphs
            .into_par_iter()
            .map(|(block, graph, baseline, return_destination, tail_call)| {
                let schedule = |graph| {
                    depth_first_search::schedule(
                        block,
                        spill_alloc_start,
                        shuffle_config,
                        depth_first_search::SearchConfig {
                            max_candidates: NonZero::new(DEFAULT_MAX_SEARCH_CANDIDATES).unwrap(),
                        },
                        graph,
                    )
                };
                let rematerialized = schedule(&graph);
                let result = match baseline {
                    None => rematerialized,
                    Some((baseline_graph, replay_bytecode_size, replay_execution_gas)) => {
                        let baseline = schedule(&baseline_graph);
                        if rematerialized_schedule_is_better(
                            &baseline.ops,
                            &rematerialized.ops,
                            replay_bytecode_size,
                            replay_execution_gas,
                            shuffle_config,
                        ) {
                            rematerialized
                        } else {
                            baseline
                        }
                    }
                };
                let mut result =
                    rematerialize_global_loads(result, &spilled_constants, shuffle_config);
                for operation in &mut result.ops {
                    let StackOps::Op(operation_id) = operation else { continue };
                    let Operation::InternalCall(call) = program.operations[*operation_id] else {
                        continue;
                    };
                    if let Some(slot) = memory_return_destinations.slot_for_function(call.function)
                    {
                        *operation = StackOps::MemoryReturnCall(*operation_id, slot);
                    }
                }
                if let Some(slot) = return_destination {
                    let mut operations = result.ops.into_vec();
                    operations.push(StackOps::Load(slot));
                    result.ops = operations.into_boxed_slice();
                }
                if let Some(tail_call) = tail_call {
                    let mut operations = result.ops.into_vec();
                    operations.push(StackOps::TailCall(tail_call));
                    result.ops = operations.into_boxed_slice();
                }
                (block.id(), result)
            })
            .collect::<Vec<_>>()
    });

    for (block_id, schedule) in block_schedules {
        let alloc_offset = next_alloc_id - local_alloc_start;
        let ops_idx = ops.push_iter(schedule.ops.into_iter().map(|op| match op {
            StackOps::Store(id) if id >= local_alloc_start => StackOps::Store(id + alloc_offset),
            StackOps::Load(id) if id >= local_alloc_start => StackOps::Load(id + alloc_offset),
            op => op,
        }));
        next_alloc_id += schedule.spill_count;
        bb_to_ops.insert(block_id, ops_idx);
    }

    (ScheduledOps { bb_to_ops, ops }, layouts, next_alloc_id)
}

#[cfg(test)]
mod tests;
