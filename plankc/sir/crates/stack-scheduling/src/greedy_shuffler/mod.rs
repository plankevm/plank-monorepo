use crate::{
    op_graph::*,
    stack::{ScheduleConfig, StackOps, TrackedStack},
};
use indices::*;
use plank_core::LoopLimit;

mod indices;

#[cfg(test)]
mod tests;

pub struct GreedyShuffler<'a, 'ir, Sink: FnMut(StackOps)> {
    correct_up_to: Height,
    current: &'a mut TrackedStack<'ir, Sink>,
    target: &'a [ValueNodeId],
    max_swap_depth: Depth<CurrentStack>,
    max_dup_depth: Depth<CurrentStack>,
}

impl<'a, 'ir, Sink: FnMut(StackOps)> GreedyShuffler<'a, 'ir, Sink> {
    pub fn run(
        current: &'a mut TrackedStack<'ir, Sink>,
        target: &'a [ValueNodeId],
        config: ScheduleConfig,
    ) {
        let mut this = Self {
            correct_up_to: Height(0),
            current,
            target,
            max_swap_depth: Depth::new(config.max_swap_depth.into()),
            max_dup_depth: Depth::new(config.max_dup_depth.into()),
        };

        println!("hey");

        this.update_correct_up_to_height();

        println!("this.correct_up_to: {:?}", this.correct_up_to);

        this.shrink_current_stack(this.max_swap_depth + 1);
        this.grow();
    }

    #[track_caller]
    fn swap(&mut self, depth: Depth<CurrentStack>) {
        let depth =
            (depth <= self.max_swap_depth).then_some(depth).and_then(|d| d.0.try_into().ok());
        self.current.swap(depth.expect("invalid depth"));
    }

    fn from_target(&self, depth: Depth<TargetStack>) -> Height {
        Height(self.target.len() - depth.0)
    }

    fn from_current(&self, depth: Depth<CurrentStack>) -> Height {
        Height(self.current.fifo().len() - depth.0)
    }

    fn to_target(&self, height: Height) -> Depth<TargetStack> {
        height.to_depth(self.target)
    }

    fn to_current(&self, height: Height) -> Depth<CurrentStack> {
        height.to_depth(self.current.fifo())
    }

    fn current_depth(&self) -> Depth<CurrentStack> {
        Depth::new(self.current.len().into())
    }

    fn target_depth(&self) -> Depth<CurrentStack> {
        Depth::new(self.target.len().into())
    }

    fn target<I: StackIndex<TargetStack>>(&self, index: I) -> I::Output<'_> {
        index.index(self.target)
    }

    fn current<I: StackIndex<CurrentStack>>(&self, index: I) -> I::Output<'_> {
        index.index(self.current.fifo())
    }

    fn update_correct_up_to_height(&mut self) {
        let mut correct_up_to = self.correct_up_to;

        let target_fifo = self.target(..correct_up_to);
        let current_fifo = self.current(..correct_up_to);

        for (&target, &current) in target_fifo.iter().rev().zip(current_fifo.iter().rev()) {
            if target != current {
                break;
            }

            if self.target(..correct_up_to + 1).contains(&target)
                && !self.current(..correct_up_to + 1).contains(&target)
            {
                break;
            }

            correct_up_to += 1;
        }

        self.correct_up_to = correct_up_to;
    }

    fn shrink_current_stack(&mut self, desired: Depth<CurrentStack>) {
        let mut limit = LoopLimit::max(100_000);
        while self.to_current(self.correct_up_to) > desired {
            limit.tick();
            let stepped = self.pop_unneeded()
                || self.swap_to_correct_position()
                || self.pop_extra()
                || self.swap_and_pop_extra()
                || self.pop_duplicate();
            if !stepped {
                self.current.spill_top();
            }
        }
    }

    fn grow(&mut self) {}

    fn is_unneeded(&self, value: ValueNodeId) -> bool {
        !self.target(..self.correct_up_to).contains(&value)
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

    fn swap_to_correct_position(&mut self) -> bool {
        let top = self.current(Depth::new(0));

        let max_search_depth = self.max_swap_depth.min(self.to_current(self.correct_up_to));
        let mut slot = self.from_current(max_search_depth);
        let top_height = self.from_current(Depth::new(0));

        while slot < top_height {
            if self.current(slot) != top && self.target(slot) == top {
                self.swap(self.to_current(slot));
                return true;
            }
            slot += 1;
        }

        false
    }

    fn is_extra(&self, value: ValueNodeId) -> bool {
        let mut target_count =
            self.target(..self.correct_up_to).iter().filter(|&&v| v == value).count();
        for &v in self.current(..self.correct_up_to) {
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
        let mut search_depth = self.max_swap_depth.min(self.to_current(self.correct_up_to));

        while Depth::new(1) <= search_depth {
            let value = self.current(search_depth);
            if self.is_extra(value) {
                self.swap(search_depth);
                self.current.pop();
                return true;
            }
            search_depth -= 1;
        }

        false
    }

    fn is_duplicate(&self, value: ValueNodeId) -> bool {
        let current_count =
            self.current(..self.correct_up_to).iter().filter(|&&v| v == value).count();
        current_count >= 2
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
}
