use crate::{
    op_graph::{OpGraph, OpNodeId, OpSet, ValueNodeId},
    stack::{ScheduleConfig, StackOps, TrackedStack},
    state::is_last_use,
};
use allocator_api2::vec::Vec;
use plank_core::LoopLimit;
use stumpalo::ArenaRef;

#[cfg(test)]
mod tests;

#[cfg(test)]
fn dedup_unsorted<T: PartialEq>(values: &mut Vec<T>) {
    let mut i = 0;
    while i < values.len() {
        let mut j = i + 1;
        while j < values.len() {
            if values[i] == values[j] {
                values.swap_remove(j);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

pub(crate) fn greedy_schedule_op<Sink: FnMut(StackOps)>(
    arena: &ArenaRef<'_>,
    config: ScheduleConfig,
    stack: &mut TrackedStack<'_, Sink>,
    graph: &OpGraph,
    op_id: OpNodeId,
    complete: OpSet<'_>,
) {
    let op = graph.get_op(op_id);

    let mut unique_last_uses = Vec::with_capacity_in(op.inputs_fifo.len() / 2, arena);
    for &value in op.inputs_fifo {
        if is_last_use(graph, complete, value) && !unique_last_uses.contains(&value) {
            unique_last_uses.push(value);
        }
    }

    let target_depth_delta = op.inputs_fifo.len() - unique_last_uses.len();
    let mut scheduler = GreedyIntraOpScheduler {
        current: stack,
        target: op.inputs_fifo,
        complete: 0,
        target_depth_delta,
        max_swap_depth: config.max_swap_depth,
        max_dup_depth: config.max_dup_depth,
    };

    scheduler.grow();

    stack.op(graph, op_id);
}

pub struct GreedyIntraOpScheduler<'a, 'ir, Sink: FnMut(StackOps)> {
    current: &'a mut TrackedStack<'ir, Sink>,
    target: &'a [ValueNodeId],
    complete: usize,
    target_depth_delta: usize,
    max_swap_depth: u8,
    max_dup_depth: u8,
}

const LIMIT: u32 = 100_000;

impl<'a, 'ir, Sink: FnMut(StackOps)> GreedyIntraOpScheduler<'a, 'ir, Sink> {
    fn grow(&mut self) {
        let mut limit = LoopLimit::max(LIMIT);
        while self.complete < self.target.len() {
            limit.tick();
            let current_incomplete = self.complete + self.target_depth_delta < self.target.len();
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

        self.current.swap(i);
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
        todo!()
    }

    fn unspill_unavailable_horizon(&mut self) -> bool {
        todo!()
    }

    fn dup_needed(&mut self) -> bool {
        todo!()
    }

    fn unspill_needed(&mut self) -> bool {
        todo!()
    }

    fn swap(&mut self, depth: u8) {
        assert!(depth <= self.max_swap_depth);
        if depth > 0 {
            self.current.swap(depth);
        }
    }

    fn dup(&mut self, depth: u8) {
        assert!(depth <= self.max_dup_depth);
        self.target_depth_delta -= 1;
        self.current.dup(depth);
    }

    fn iter_pairwise<'s>(
        &'s self,
        max_depth: u8,
    ) -> impl Iterator<Item = (ValueNodeId, ValueNodeId, u8)> {
        let total_incomplete = self.target.len() - self.complete - self.target_depth_delta;
        let lined_up_target = &self.target[self.target_depth_delta..];
        let current = self.current.fifo().iter();

        current
            .zip(lined_up_target)
            .zip(0..=max_depth)
            .take(total_incomplete)
            .rev()
            .map(|((&current, &target), i)| (current, target, i))
    }
}
