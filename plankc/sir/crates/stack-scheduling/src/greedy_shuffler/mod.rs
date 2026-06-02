use crate::{
    op_graph::*,
    stack::{ScheduleConfig, StackOps, TrackedStack},
};
use indices::*;
use plank_core::{IncIterable, LoopLimit, span::ToUsize};

mod indices;

#[cfg(test)]
mod tests;

pub struct GreedyShuffler<'a, 'ir, Sink: FnMut(StackOps)> {
    complete_at_bottom: usize,
    current: &'a mut TrackedStack<'ir, Sink>,
    target: &'a [ValueNodeId],
    max_swap_depth: Depth<CurrentStack>,
    max_dup_depth: Depth<CurrentStack>,
}

const TOP_IDX: Depth<CurrentStack> = Depth::new(0);

pub fn shuffle<'a, 'ir, Sink: FnMut(StackOps)>(
    config: ScheduleConfig,
    current: &'a mut TrackedStack<'ir, Sink>,
    graph: &'a OpGraph,
) {
    GreedyShuffler::run(config, current, graph.end_stack_fifo.as_slice());
}

impl<'a, 'ir, Sink: FnMut(StackOps)> GreedyShuffler<'a, 'ir, Sink> {
    pub fn run(
        config: ScheduleConfig,
        current: &'a mut TrackedStack<'ir, Sink>,
        target: &'a [ValueNodeId],
    ) {
        let mut this = Self {
            complete_at_bottom: 0,
            current,
            target,
            max_swap_depth: Depth::new(config.max_swap_depth.into()),
            max_dup_depth: Depth::new(config.max_dup_depth.into()),
        };

        this.update_correct();
        this.shrink();
        this.grow();
        this.cleanup_unneeded_top();
    }

    fn cleanup_unneeded_top(&mut self) {
        while self.current.len().to_usize() > self.target.len() {
            self.current.pop();
        }
    }

    #[track_caller]
    fn swap(&mut self, depth: Depth<CurrentStack>) {
        if depth == Depth::new(0) {
            return;
        }
        let depth =
            (depth <= self.max_swap_depth).then_some(depth).and_then(|d| d.0.try_into().ok());
        self.current.swap(depth.expect("invalid depth"));
    }

    #[track_caller]
    fn dup(&mut self, depth: Depth<CurrentStack>) {
        let depth =
            (depth <= self.max_dup_depth).then_some(depth).and_then(|d| d.0.try_into().ok());
        self.current.dup(depth.expect("invalid depth"));
    }

    #[track_caller]
    fn from_current(&self, depth: Depth<CurrentStack>) -> FromBottom {
        FromBottom(self.current.fifo().len() - depth.0 - 1)
    }

    #[track_caller]
    fn to_current(&self, depth: FromBottom) -> Depth<CurrentStack> {
        Depth::new(self.current.fifo().len() - depth.0 - 1)
    }

    fn current_len(&self) -> Depth<CurrentStack> {
        Depth::new(self.current.len().into())
    }

    #[track_caller]
    fn target<I: StackIndex<TargetStack>>(&self, index: I) -> I::Output<'_> {
        index.index(self.target)
    }

    #[track_caller]
    fn current<I: StackIndex<CurrentStack>>(&self, index: I) -> I::Output<'_> {
        index.index(self.current.fifo())
    }

    fn update_correct(&mut self) {
        let mut newly_complete = 0;
        // If `0` complete we want all values including the bottom most (`..=FromBottom(0)`), if
        // `1` is complete we want to skip the bottom most value, giving us the range
        // `..=FromBottom(1)` and so on.
        for (current, target, i) in self.iter_bottom_up(FromBottom(self.complete_at_bottom)) {
            if current != target {
                break;
            }

            let needed_further_up = self.target(..i).contains(&target);
            if needed_further_up {
                // Determining whether it's worth retrieving a needed value by unspilling is
                // deferred to the rest of the algorithm which is why spilled is not checked.
                let another_copy_exists_further_up = self.current(..i).contains(&target);
                if !another_copy_exists_further_up {
                    break;
                }
            }

            newly_complete += 1;
        }

        self.complete_at_bottom += newly_complete;
    }

    fn shrink(&mut self) {
        let mut limit = LoopLimit::max(100_000);

        let can_access_length = self.max_swap_depth + 1;
        while {
            let need_access_length = self.current_len() - self.complete_at_bottom;
            can_access_length < need_access_length
        } {
            limit.tick();
            let stepped = self.pop_unneeded()
                || self.swap_to_correct_position()
                || self.pop_extra()
                || self.swap_and_pop_extra()
                || self.pop_duplicate();
            if !stepped {
                self.current.spill_top();
            }
            self.update_correct();
        }
    }

    fn grow(&mut self) {
        let mut limit = LoopLimit::max(100_000);
        while self.complete_at_bottom < self.target.len() {
            limit.tick();
            let current_contains_incomplete =
                self.current.len().to_usize() > self.complete_at_bottom;
            let stepped = current_contains_incomplete
                && (self.pop_unneeded()
                    || self.swap_to_correct_position()
                    || self.exchange_via_top()
                    || self.pop_extra());
            if !stepped {
                if self.can_push() {
                    assert!(self.dup_needed() || self.unspill_needed());
                } else {
                    self.current.spill_top();
                }
            }
            self.update_correct();
        }
    }

    fn is_unneeded(&self, value: ValueNodeId) -> bool {
        !self.target(..=FromBottom(self.complete_at_bottom)).contains(&value)
    }

    fn pop_unneeded(&mut self) -> bool {
        let top = self.current(Depth::new(0));
        if self.is_unneeded(top) {
            self.current.pop();
            true
        } else {
            false
        }
    }

    #[track_caller]
    fn iter_bottom_up<'s>(
        &'s self,
        mut inclusive: FromBottom,
    ) -> impl Iterator<Item = (ValueNodeId, ValueNodeId, FromBottom)> + 's {
        self.current(..=inclusive).iter().rev().zip(self.target(..=inclusive).iter().rev()).map(
            move |(&current_value, &target_value)| {
                (current_value, target_value, inclusive.get_and_inc())
            },
        )
    }

    fn swap_to_correct_position(&mut self) -> bool {
        let top = self.current(TOP_IDX);

        let max_search_depth =
            self.max_swap_depth.min(self.to_current(FromBottom(self.complete_at_bottom)));

        let swap_idx = 'swap_idx: {
            for (current, target, i) in self.iter_bottom_up(self.from_current(max_search_depth)) {
                if self.to_current(i) == TOP_IDX {
                    break 'swap_idx None;
                }
                if current != top && target == top {
                    break 'swap_idx Some(i);
                }
            }
            None
        };

        if let Some(idx) = swap_idx {
            self.swap(self.to_current(idx));
            return true;
        }

        false
    }

    fn is_extra(&self, value: ValueNodeId) -> bool {
        let maybe_incorrect_inclusive = FromBottom(self.complete_at_bottom);
        let mut target_count =
            self.target(..=maybe_incorrect_inclusive).iter().filter(|&&v| v == value).count();
        for &v in self.current(..=maybe_incorrect_inclusive) {
            if v == value {
                if target_count == 0 {
                    return true;
                }
                target_count -= 1;
            }
        }
        false
    }

    fn pop_extra(&mut self) -> bool {
        let top = self.current(Depth::new(0));
        if self.is_extra(top) {
            self.current.pop();
            true
        } else {
            false
        }
    }

    fn swap_and_pop_extra(&mut self) -> bool {
        let max_search_depth =
            self.max_swap_depth.min(self.to_current(FromBottom(self.complete_at_bottom)));

        let swap_idx = 'swap_idx: {
            for (current, _target, i) in self.iter_bottom_up(self.from_current(max_search_depth)) {
                if self.to_current(i) == TOP_IDX {
                    break;
                }

                if self.is_extra(current) {
                    break 'swap_idx Some(self.to_current(i));
                }
            }
            None
        };

        if let Some(swap_idx) = swap_idx {
            self.swap(swap_idx);
            self.current.pop();
            return true;
        }

        false
    }

    fn is_duplicate(&self, value: ValueNodeId) -> bool {
        let mut current_count = 0;
        for &v in self.current(..=FromBottom(self.complete_at_bottom)) {
            if v == value {
                current_count += 1;
                if current_count >= 2 {
                    return true;
                }
            }
        }
        false
    }

    fn pop_duplicate(&mut self) -> bool {
        let top = self.current(Depth::new(0));
        if self.is_duplicate(top) {
            self.current.pop();
            true
        } else {
            false
        }
    }

    fn exchange_via_top(&mut self) -> bool {
        let max_swap_depth = self.from_current(
            self.max_swap_depth.min(self.to_current(FromBottom(self.complete_at_bottom))),
        );

        let exchange =
            self.iter_bottom_up(max_swap_depth).find_map(|(current, target, dest_idx)| {
                if current == target {
                    return None;
                }

                let src_idx = self.iter_bottom_up(max_swap_depth).find_map(
                    |(src, target_at_src, src_idx)| {
                        (src != target_at_src && src == target).then_some(src_idx)
                    },
                )?;

                Some((src_idx, dest_idx))
            });

        if let Some((src_idx, dst_idx)) = exchange {
            let src_idx = self.to_current(src_idx);
            let dst_idx = self.to_current(dst_idx);
            self.swap(src_idx);
            self.swap(dst_idx);
            return true;
        }

        false
    }

    fn can_push(&self) -> bool {
        if self.current_len() <= self.max_swap_depth {
            // Can grow because bottom will remain accessible if grown by 1.
            return true;
        }

        assert!(
            self.current.len().to_usize() <= self.complete_at_bottom
                || self.to_current(FromBottom(self.complete_at_bottom)) <= self.max_swap_depth
        );
        let horizon_idx = self.from_current(self.max_swap_depth);
        let value = self.target(horizon_idx);
        let current = self.current(horizon_idx);

        if current != value {
            return false;
        }

        let needed_further_up = self.target(..horizon_idx).contains(&value);
        if needed_further_up {
            let another_copy_accessible = self.current(..horizon_idx).contains(&value)
                || self.current.get_spilled(value).is_some();
            if !another_copy_accessible {
                return false;
            }
        }

        true
    }

    fn dup_needed(&mut self) -> bool {
        if self.current.is_empty() {
            return false;
        }

        let max_dup_depth = self.max_dup_depth.min(self.current_len() - 1);

        let search_depth = self.from_current(max_dup_depth);
        let dup_idx = self.iter_bottom_up(search_depth).find_map(|(current, _target, i)| {
            let required_copies =
                self.target(..=search_depth).iter().filter(|&&v| v == current).count();
            let available_copies =
                self.current(..=search_depth).iter().filter(|&&v| v == current).count();
            (available_copies < required_copies).then(|| self.to_current(i))
        });

        if let Some(dup_idx) = dup_idx {
            self.dup(dup_idx);
            return true;
        }

        false
    }

    fn unspill_needed(&mut self) -> bool {
        let max_dup_depth_exclusive = (self.max_dup_depth + 1).min(self.current_len());
        for &value in self.target(..=FromBottom(self.complete_at_bottom)).iter().rev() {
            if !self.current(..max_dup_depth_exclusive).contains(&value) {
                self.current.unspill(value);
                return true;
            }
        }

        false
    }
}
