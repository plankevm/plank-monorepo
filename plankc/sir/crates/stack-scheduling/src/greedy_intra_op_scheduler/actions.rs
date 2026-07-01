use crate::{
    greedy_intra_op_scheduler::state::Candidate,
    stack::{StackOps, TrackedStack},
};

pub fn possible_actions(candidate: &Candidate) -> impl Iterator<Item = (StackOps)> {
    [].into_iter()
}
