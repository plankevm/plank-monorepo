use crate::{
    op_graph::*,
    stack::{EvmStack, ScheduleConfig, StackOps, TrackedStack},
};
use plank_core::LoopLimit;
use sir_data::{BlockView, ControlView, IndexVec, StaticAllocId, index_vec};
use std::{cell::Cell, collections::VecDeque};

pub struct GreedyShuffler<'a, 'ir, Sink: FnMut(StackOps)> {
    correct_up_to_height: usize,
    current: &'a mut TrackedStack<'ir, Sink>,
    target: &'a [ValueNodeId],
    max_swap_depth: u8,
    max_dup_depth: u8,
}

impl<'a, 'ir, Sink: FnMut(StackOps)> GreedyShuffler<'a, 'ir, Sink> {
    pub fn run(
        current: &'a mut TrackedStack<'ir, Sink>,
        target: &'a [ValueNodeId],
        config: ScheduleConfig,
    ) {
        let mut this = Self {
            correct_up_to_height: 0,
            current,
            target,
            max_swap_depth: config.max_swap_depth,
            max_dup_depth: config.max_dup_depth,
        };

        this.update_correct_up_to_height();
        this.shrink_current_stack(usize::from(this.max_swap_depth) + 1);
    }

    fn update_correct_up_to_height(&mut self) {
        let mut target_correct_up_to_depth = self.target.len() - self.correct_up_to_height;
        let mut current_correct_up_to_depth =
            self.current.len() as usize - self.correct_up_to_height;

        let target_fifo = &self.target[..target_correct_up_to_depth];
        let current_fifo = &self.current.fifo()[..current_correct_up_to_depth];

        for (&target, &current) in target_fifo.iter().rev().zip(current_fifo.iter().rev()) {
            if target == current {
                let still_needed_higher_up =
                    target_fifo[..target_correct_up_to_depth.saturating_sub(1)].contains(&target);
                if still_needed_higher_up {
                    let current_has_another_copy_higher_up = current_fifo
                        [..current_correct_up_to_depth.saturating_sub(1)]
                        .contains(&target);
                    if !current_has_another_copy_higher_up {
                        break;
                    }
                }

                target_correct_up_to_depth -= 1;
                current_correct_up_to_depth -= 1;
                self.correct_up_to_height += 1;
                continue;
            }
            break;
        }
    }

    fn shrink_current_stack(&mut self, current_stack_desired_len: usize) {
        let mut limit = LoopLimit::max(100_000);
        while self.current.len() as usize - self.correct_up_to_height > current_stack_desired_len {
            limit.tick();
            let stepped = self.pop_unneeded()
                || self.swap_to_correct_position()
                || self.pop_extra()
                || self.swap_and_pop_extra()
                || self.pop_duplicate()
                || self.spill();
            assert!(stepped, "not done but can't step");
        }
    }
}
