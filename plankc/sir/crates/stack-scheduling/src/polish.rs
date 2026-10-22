//! Post-pass over a finished schedule: every maximal run of swaps, dups and pops between two
//! other operations moves the stack from one exact state to another. The greedy operand
//! preparer builds that state one operand at a time; re-solving the whole transition with the
//! shuffler sometimes finds a cheaper sequence for the same end state.

use crate::{
    greedy_shuffler,
    op_graph::{OpGraph, OpNodeKind, ValueNodeId},
    stack::{EvmStack, ShuffleConfig, StackOps, TrackedStack, gas_cost},
};
use plank_core::Idx;
use sir_data::StaticAllocId;

pub fn polish(
    graph: &OpGraph,
    config: ShuffleConfig,
    first_spill: StaticAllocId,
    schedule: &[StackOps],
) -> Box<[StackOps]> {
    let mut stack = EvmStack::new();
    for input in graph.input_values_fifo().iter().rev() {
        stack.push(input);
    }
    let mut spilled = Vec::<ValueNodeId>::new();
    let mut polished = Vec::with_capacity(schedule.len());
    let mut segment_start = stack.clone();
    let mut segment = Vec::new();

    for &operation in schedule {
        match operation {
            StackOps::Swap(depth) => stack.swap_with_top(depth.into()),
            StackOps::Dup(depth) => stack.duplicate(depth.into()),
            StackOps::Pop => {
                stack.pop().expect("pop on empty stack");
            }
            StackOps::Exchange(_, _)
            | StackOps::Store(_)
            | StackOps::Load(_)
            | StackOps::Op(_)
            | StackOps::Flipped(_)
            | StackOps::CallRetPush(_) => {
                flush_segment(config, &segment_start, &stack, &mut segment, &mut polished);
                apply_other(graph, first_spill, &mut stack, &mut spilled, operation);
                polished.push(operation);
                segment_start = stack.clone();
                continue;
            }
        }
        segment.push(operation);
    }
    flush_segment(config, &segment_start, &stack, &mut segment, &mut polished);
    polished.into_boxed_slice()
}

fn flush_segment(
    config: ShuffleConfig,
    start: &EvmStack,
    end: &EvmStack,
    segment: &mut Vec<StackOps>,
    polished: &mut Vec<StackOps>,
) {
    if segment.len() > 1 {
        let mut candidate = Vec::new();
        let mut shuffled = TrackedStack::new_from_parts(
            StaticAllocId::ZERO,
            |op| candidate.push(op),
            start.fifo(),
            Vec::new(),
        );
        greedy_shuffler::shuffle_to(config, &mut shuffled, end.fifo(), true);
        let reaches_end = shuffled.fifo() == end.fifo();
        drop(shuffled);
        let only_stack_ops = candidate
            .iter()
            .all(|op| matches!(op, StackOps::Swap(_) | StackOps::Dup(_) | StackOps::Pop));
        if reaches_end && only_stack_ops && gas_cost(&candidate, config) < gas_cost(segment, config)
        {
            *segment = candidate;
        }
    }
    polished.append(segment);
}

fn apply_other(
    graph: &OpGraph,
    first_spill: StaticAllocId,
    stack: &mut EvmStack,
    spilled: &mut Vec<ValueNodeId>,
    operation: StackOps,
) {
    let (operation_idx, ret_dest_push) = match operation {
        StackOps::Store(alloc) => {
            let slot = (alloc - first_spill) as usize;
            if spilled.len() <= slot {
                spilled.resize(slot + 1, ValueNodeId::ZERO);
            }
            spilled[slot] = stack.pop().expect("store on empty stack");
            return;
        }
        StackOps::Load(alloc) => {
            stack.push(spilled[(alloc - first_spill) as usize]);
            return;
        }
        StackOps::Exchange(n, m) => {
            stack.exchange(n.into(), m.into());
            return;
        }
        StackOps::Op(idx) | StackOps::Flipped(idx) => (idx, false),
        StackOps::CallRetPush(idx) => (idx, true),
        StackOps::Swap(_) | StackOps::Dup(_) | StackOps::Pop => {
            unreachable!("plain stack ops are handled by the caller")
        }
    };
    let op = graph
        .op_ids()
        .map(|op| graph.get_op(op))
        .find(|op| match op.kind {
            OpNodeKind::Flippable(idx) | OpNodeKind::Normal(idx) => {
                !ret_dest_push && idx == operation_idx
            }
            OpNodeKind::RetDestPush(idx) => ret_dest_push && idx == operation_idx,
        })
        .expect("scheduled operation missing from graph");
    for _ in op.inputs_fifo {
        stack.pop().expect("missing input");
    }
    for &output in op.outputs_fifo.iter().rev() {
        stack.push(output);
    }
}
