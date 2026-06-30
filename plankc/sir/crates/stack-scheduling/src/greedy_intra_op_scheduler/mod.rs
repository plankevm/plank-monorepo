use crate::{
    op_graph::{OpGraph, OpNodeId, OpNodeKind, OpSet, ValueNodeId},
    stack::{ScheduleConfig, StackOps, TrackedStack},
};
use allocator_api2::vec::Vec;
use plank_core::LoopLimit;
use stumpalo::ArenaRef;

mod state;

#[cfg(test)]
mod tests;

/*
 * # Search State
 * - stack
 * - spilled
 * - cost so far
 * - list of actions
 */

pub(crate) fn greedy_schedule_op<Sink: FnMut(StackOps)>(
    arena: &ArenaRef<'_>,
    config: ScheduleConfig,
    stack: &mut TrackedStack<Sink>,
    graph: &OpGraph,
    op_id: OpNodeId,
    complete: OpSet<'_>,
    beam_width: u16,
) {
    let op = graph.get_op(op_id);
    let flippable = matches!(op.kind, OpNodeKind::Flippable(_));

    let mut unique_last_uses = Vec::with_capacity_in(op.inputs_fifo.len() / 2, arena);
    for &value in op.inputs_fifo {
        if graph.is_last_use(complete, value) && !unique_last_uses.contains(&value) {
            unique_last_uses.push(value);
        }
    }

    let flipped = false; // TODO

    stack.op(graph, op_id, flipped);
}

pub struct GreedyIntraOpScheduler<'a, Sink: FnMut(StackOps)> {
    current: &'a mut TrackedStack<Sink>,
    target: &'a [ValueNodeId],
    complete: u16,
    target_depth_delta: usize,
    max_swap_depth: u8,
    max_dup_depth: u8,
    last_uses: Vec<ValueNodeId, &'a ArenaRef<'a>>,
}

const LIMIT: u32 = 100_000;

impl<'a, Sink: FnMut(StackOps)> GreedyIntraOpScheduler<'a, Sink> {
    fn update_complete(&mut self) {
        let mut newly_complete = 0;
        for (current, target, i) in self.iter_pairwise(u16::MAX) {
            if current != target {
                break;
            }

            let needed_further_up = self.lined_up_target()[..i as usize].contains(&target);
            if needed_further_up {
                let another_copy_exists_further_up = self.current.fifo()[..i as usize]
                    .contains(&target)
                    || self.current.get_spilled(target).is_some();
                if !another_copy_exists_further_up {
                    break;
                }
            }

            newly_complete += 1;
        }

        self.complete += newly_complete;
    }

    fn grow(&mut self) {
        let mut limit = LoopLimit::max(LIMIT);
        let complete = usize::from(self.complete);
        while complete < self.target.len() {
            limit.tick();
            let current_incomplete = complete + self.target_depth_delta < self.target.len();
            let stepped =
                current_incomplete && (self.swap_to_correct_position() || self.exchange_via_top());

            if !stepped {
                if self.can_push() {
                    let pushed = self.unspill_unavailable_horizon()
                        || self.dup_needed()
                        || self.unspill_needed();
                    assert!(pushed);
                } else {
                    self.current.spill_top();
                }
            }
        }
    }

    fn swap_to_correct_position(&mut self) -> bool {
        let &top = self.current.fifo().first().expect("should have top if current incomplete");

        let Some((_, _, i)) = self
            .iter_pairwise(self.max_swap_depth)
            .find(|&(target, current, i)| target != current && target == top && i > 0)
        else {
            return false;
        };

        self.swap(i);
        true
    }

    fn exchange_via_top(&mut self) -> bool {
        let exchange = self
            .iter_pairwise(self.max_swap_depth)
            .filter(|(current_at_dst, target, _dst_idx)| current_at_dst != target)
            .find_map(|(_current_at_dst, target, dst_idx)| {
                self.iter_pairwise(self.max_swap_depth).find_map(|(src, target_at_src, src_idx)| {
                    (src != target_at_src && src == target).then_some((src_idx, dst_idx))
                })
            });

        let Some((src_idx, dst_idx)) = exchange else { return false };

        self.swap(src_idx);
        self.swap(dst_idx);

        true
    }

    fn can_push(&self) -> bool {
        self.reachable_incomplete() <= usize::from(self.max_swap_depth)
    }

    fn unspill_unavailable_horizon(&mut self) -> bool {
        let max_swap_depth = usize::from(self.max_swap_depth);
        if self.reachable_incomplete() < max_swap_depth {
            return false;
        }

        let horizon_idx = max_swap_depth - 1;
        let target = self.lined_up_target()[horizon_idx];
        let current = self.current.fifo()[horizon_idx];
        if target != current && !self.current.fifo()[..max_swap_depth].contains(&target) {
            self.current.unspill(target);
            return true;
        }

        false
    }

    fn dup_needed(&mut self) -> bool {
        todo!()
    }

    fn unspill_needed(&mut self) -> bool {
        todo!()
    }

    fn swap(&mut self, depth: u16) {
        assert!(depth <= u16::from(self.max_swap_depth));
        if depth > 0 {
            self.current.swap(depth as u8);
        }
    }

    fn dup(&mut self, depth: u8) {
        assert!(depth <= self.max_dup_depth);
        self.target_depth_delta -= 1;
        self.current.dup(depth);
    }

    fn lined_up_target(&self) -> &[ValueNodeId] {
        &self.target[self.target_depth_delta..]
    }

    /// The number of values that are incomplete but reachable given the current depth.
    fn reachable_incomplete(&self) -> usize {
        self.target.len() - usize::from(self.complete) - self.target_depth_delta
    }

    /// Yields `(current, target, i)` up to depth `max_depth`.
    fn iter_pairwise<'s>(
        &'s self,
        max_depth: impl Into<u16>,
    ) -> impl Iterator<Item = (ValueNodeId, ValueNodeId, u16)> {
        self.current
            .fifo()
            .iter()
            .zip(self.lined_up_target())
            .zip(0..=max_depth.into())
            .take(self.reachable_incomplete())
            .rev()
            .map(|((&current, &target), i)| (current, target, i))
    }
}
