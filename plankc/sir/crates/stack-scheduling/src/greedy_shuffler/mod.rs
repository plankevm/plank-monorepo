use crate::{
    op_graph::*,
    stack::{ScheduleConfig, StackOps, TrackedStack},
};
use indices::*;
use plank_core::{IncIterable, LoopLimit, Span, span::ToUsize};

mod indices;

#[cfg(test)]
mod tests;

pub struct GreedyShuffler<'a, 'ir, Sink: FnMut(StackOps)> {
    complete_at_bottom: usize,
    current: &'a mut TrackedStack<'ir, Sink>,
    target: &'a [ValueNodeId],
    max_swap_depth: FromTop<CurrentStack>,
    max_dup_depth: FromTop<CurrentStack>,
}

pub fn shuffle<'a, 'ir, Sink: FnMut(StackOps)>(
    config: ScheduleConfig,
    current: &'a mut TrackedStack<'ir, Sink>,
    graph: &'a OpGraph,
) {
    GreedyShuffler::run(config, current, graph.output_values_fifo());
}

const LIMIT: u32 = 100_000;

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
            max_swap_depth: FromTop::new(config.max_swap_depth.into()),
            max_dup_depth: FromTop::new(config.max_dup_depth.into()),
        };

        this.update_complete_at_bottom();
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
    fn swap(&mut self, i: FromTop<CurrentStack>) {
        if i == FromTop::new(0) {
            return;
        }
        assert!(i <= self.max_swap_depth, "invalid swap depth");
        self.current.swap(i.0.try_into().expect("overflow despite assert"));
    }

    #[track_caller]
    fn dup(&mut self, i: FromTop<CurrentStack>) {
        assert!(i <= self.max_dup_depth, "invalid dup depth");
        self.current.dup(i.0.try_into().expect("overflow despite assert"));
    }

    #[track_caller]
    fn target_to(&self, i: FromTop<TargetStack>) -> FromBottom {
        FromBottom(self.target.len() - i.0 - 1)
    }

    #[track_caller]
    fn current_to(&self, i: FromTop<CurrentStack>) -> FromBottom {
        FromBottom(self.current.fifo().len() - i.0 - 1)
    }

    #[track_caller]
    fn to_current(&self, i: FromBottom) -> FromTop<CurrentStack> {
        FromTop::new(self.current.fifo().len() - i.0 - 1)
    }

    fn current_len(&self) -> FromTop<CurrentStack> {
        FromTop::new(self.current.len().into())
    }

    #[track_caller]
    fn target<I: StackIndex<TargetStack>>(&self, index: I) -> I::Output<'_> {
        index.index(self.target)
    }

    #[track_caller]
    fn current<I: StackIndex<CurrentStack>>(&self, index: I) -> I::Output<'_> {
        index.index(self.current.fifo())
    }

    fn update_complete_at_bottom(&mut self) {
        let mut newly_complete = 0;
        // If `0` complete we want all values including the bottom most (`..=FromBottom(0)`), if
        // `1` is complete we want to skip the bottom most value, giving us the range
        // `..=FromBottom(1)` and so on.
        for (current, target, i) in self.iter_pairwise(FromBottom(self.complete_at_bottom)) {
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
        let mut limit = LoopLimit::max(LIMIT);

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
            self.update_complete_at_bottom();
        }
    }

    fn grow(&mut self) {
        let mut limit = LoopLimit::max(LIMIT);
        while self.complete_at_bottom < self.target.len() {
            limit.tick();
            let current_incomplete = self.current.len().to_usize() > self.complete_at_bottom;
            let stepped = if current_incomplete {
                self.pop_unneeded()
                    || self.swap_to_correct_position()
                    || self.exchange_via_top()
                    || self.pop_extra()
            } else {
                false
            };
            if !stepped {
                if self.can_push() {
                    assert!(
                        self.unspill_unavailable_horizon()
                            || self.dup_needed()
                            || self.unspill_needed()
                    );
                } else {
                    self.current.spill_top();
                }
            }
            self.update_complete_at_bottom();
        }
    }

    fn is_unneeded(&self, value: ValueNodeId) -> bool {
        if self.target.len() == self.complete_at_bottom {
            return true;
        }
        !self.target(..=FromBottom(self.complete_at_bottom)).contains(&value)
    }

    fn pop_unneeded(&mut self) -> bool {
        let top = self.current(FromTop::new(0));
        if self.is_unneeded(top) {
            self.current.pop();
            true
        } else {
            false
        }
    }

    #[track_caller]
    fn iter_pairwise<'s>(
        &'s self,
        mut bottom_up_start: FromBottom,
    ) -> impl Iterator<Item = (ValueNodeId, ValueNodeId, FromBottom)> + 's {
        let current = if self.current.is_empty() || {
            let highest_from_bottom = self.current_to(FromTop::new(0));
            highest_from_bottom < bottom_up_start
        } {
            &[]
        } else {
            self.current(..=bottom_up_start)
        };
        let target = if self.target.is_empty() || {
            let highest_from_bottom = self.target_to(FromTop::new(0));
            highest_from_bottom < bottom_up_start
        } {
            &[]
        } else {
            self.target(..=bottom_up_start)
        };

        current.iter().rev().zip(target.iter().rev()).map(move |(&current_value, &target_value)| {
            (current_value, target_value, bottom_up_start.get_and_inc())
        })
    }

    fn swap_to_correct_position(&mut self) -> bool {
        if self.current.len().to_usize() <= self.complete_at_bottom {
            return false;
        }

        let top = self.current(FromTop::new(0));

        let max_search_depth = self
            .max_swap_depth
            .min(self.to_current(FromBottom(self.complete_at_bottom)))
            .min(self.current_len() - 1);

        let swap_idx = self
            .iter_pairwise(self.current_to(max_search_depth))
            .find_map(|(current, target, i)| (current != top && target == top).then_some(i));

        if let Some(idx) = swap_idx {
            self.swap(self.to_current(idx));
            return true;
        }

        false
    }

    fn is_extra(&self, value: ValueNodeId) -> bool {
        let first_incorrect_from_bottom = FromBottom(self.complete_at_bottom);
        let target_count =
            self.target(..=first_incorrect_from_bottom).iter().filter(|&&v| v == value).count();
        let current_count =
            self.current(..=first_incorrect_from_bottom).iter().filter(|&&v| v == value).count();
        current_count > target_count
    }

    fn pop_extra(&mut self) -> bool {
        let top = self.current(FromTop::new(0));
        if self.is_extra(top) {
            self.current.pop();
            true
        } else {
            false
        }
    }

    fn swap_and_pop_extra(&mut self) -> bool {
        if self.current.is_empty() {
            return false;
        }

        let max_search_depth = self
            .max_swap_depth
            .min(self.to_current(FromBottom(self.complete_at_bottom)))
            .min(self.current_len() - 1);
        let mut idx = self.current_to(max_search_depth);

        let swap_idx = self
            .current(..=idx)
            .iter()
            .rev()
            .find_map(|&value| self.is_extra(value).then_some(idx.get_and_inc()));

        if let Some(swap_idx) = swap_idx {
            self.swap(self.to_current(swap_idx));
            self.current.pop();
            return true;
        }

        false
    }

    fn is_duplicate(&self, value: ValueNodeId) -> bool {
        let current_count = self
            .current(..=FromBottom(self.complete_at_bottom))
            .iter()
            .filter(|&&v| v == value)
            .count();
        current_count >= 2
    }

    fn pop_duplicate(&mut self) -> bool {
        let top = self.current(FromTop::new(0));
        if self.is_duplicate(top) {
            self.current.pop();
            true
        } else {
            false
        }
    }

    fn exchange_via_top(&mut self) -> bool {
        if self.current.is_empty() {
            return false;
        }

        let max_swap_depth = self.current_to(
            self.max_swap_depth
                .min(self.to_current(FromBottom(self.complete_at_bottom)))
                .min(self.current_len() - 1),
        );

        let exchange =
            self.iter_pairwise(max_swap_depth).find_map(|(current, target, dest_idx)| {
                if current == target {
                    return None;
                }

                let src_idx = self.iter_pairwise(max_swap_depth).find_map(
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

        let horizon_idx = self.current_to(self.max_swap_depth);
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

    fn unspill_unavailable_horizon(&mut self) -> bool {
        if self.current_len() < self.max_swap_depth {
            // We could push at least 2 values and the horizon would still remain accessible
            // via swaps.
            return false;
        }

        let horizon_idx = self.current_to(self.max_swap_depth - 1);
        let target = self.target(horizon_idx);
        let current = self.current(horizon_idx);
        if target != current && !self.current(..self.max_swap_depth).contains(&target) {
            self.current.unspill(target);
            return true;
        }

        false
    }

    fn dup_needed(&mut self) -> bool {
        if self.current.is_empty() {
            return false;
        }

        let max_dup_depth = self.max_dup_depth.min(self.current_len() - 1);

        let search_depth = self.current_to(max_dup_depth);
        let dup_idx = self.iter_pairwise(search_depth).find_map(|(_current, target, _i)| {
            let required_copies =
                self.target(..=search_depth).iter().filter(|&&v| v == target).count();

            let mut available_copies = 0;
            let mut dup_idx = None;
            for i in Span::new(FromTop::new(0), self.to_current(search_depth) + 1).iter() {
                if self.current(i) == target {
                    available_copies += 1;
                    dup_idx = dup_idx.or(Some(i));
                }
            }
            dup_idx.filter(|_| available_copies < required_copies)
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
