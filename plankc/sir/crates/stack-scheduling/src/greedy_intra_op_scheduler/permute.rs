use crate::op_graph::ValueNodeId;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueLoc {
    Tail,
    Head(ValueNodeId),
}

impl ValueLoc {
    fn is_head_and_eq(self, value: ValueNodeId) -> bool {
        matches!(self, ValueLoc::Head(v) if v == value)
    }
}

#[derive(Clone, Copy)]
struct Swap {
    accepts: ValueLoc,
    pos: u16,
}

#[derive(Clone)]
struct State {
    todo_preserve: Vec<ValueNodeId>,
    swaps: Vec<Swap>,
    executed: Vec<u16>,
}

impl State {
    fn advance(&self, loc: ValueLoc, swap_idx: usize) -> Option<Self> {
        let swap = self.swaps[swap_idx];
        let mut next = match (loc, swap.accepts) {
            (l1, l2) if l1 == l2 => self.clone(),
            (ValueLoc::Head(value), ValueLoc::Tail) => {
                let preserve_idx =
                    self.todo_preserve.iter().position(|&preserve| preserve == value)?;
                let mut next = self.clone();
                next.todo_preserve.swap_remove(preserve_idx);
                next
            }
            _ => return None,
        };
        next.swaps.swap_remove(swap_idx);
        next.executed.push(swap.pos);
        Some(next)
    }
}

pub(crate) fn permute_for_head(
    todo_preserve: &[ValueNodeId],
    head_end: usize,
    current: &[ValueNodeId],
    target: &[ValueNodeId],
    last_uses: &[ValueNodeId],
    node_budget: usize,
) -> (Vec<u16>, bool) {
    assert!(head_end > 0, "head must contain the stack top");
    assert!(head_end <= current.len(), "head exceeds current stack");
    assert!(head_end <= target.len(), "head exceeds target stack");

    let current_len = u16::try_from(current.len()).expect("stack length exceeds u16");
    let head_end_u16 = u16::try_from(head_end).expect("head length exceeds u16");
    let target_depth_delta = target.len() - head_end;
    let (head, tail) = current.split_at(head_end);
    let head_target = &target[target_depth_delta..];
    let mut swaps = Vec::with_capacity(head_end);
    let mut current_as_locs = Vec::with_capacity(current.len());

    for ((pos, &current), &target_value) in (0..head_end_u16).zip(head).zip(head_target) {
        let mut loc = ValueLoc::Head(current);
        if current != target_value {
            if pos != 0 {
                swaps.push(Swap { accepts: ValueLoc::Head(target_value), pos });
            }
            if !target.contains(&current) {
                loc = ValueLoc::Tail;
            }
        }
        current_as_locs.push(loc);
    }
    for (pos, &current) in (head_end_u16..current_len).zip(tail) {
        current_as_locs.push(ValueLoc::Head(current));
        if last_uses.contains(&current) && head_target.contains(&current) {
            swaps.push(Swap { accepts: ValueLoc::Tail, pos });
        }
    }

    let start_loc = current_as_locs[0];
    if start_loc.is_head_and_eq(head_target[0]) {
        return (Vec::new(), true);
    }

    let start_state = State { swaps, todo_preserve: todo_preserve.to_vec(), executed: Vec::new() };
    let mut pending = vec![(start_state, start_loc)];
    let mut longest = Vec::new();
    let mut longest_ends_in_top = false;

    for _ in 0..node_budget {
        let Some((state, loc)) = pending.pop() else {
            break;
        };

        let top_correct = loc.is_head_and_eq(head_target[0]);
        if state.executed.len() > longest.len() {
            longest = state.executed.clone();
            longest_ends_in_top = top_correct;
        }
        if top_correct {
            continue;
        }

        for swap_idx in (0..state.swaps.len()).rev() {
            let Some(next) = state.advance(loc, swap_idx) else { continue };
            let swap = state.swaps[swap_idx];
            pending.push((next, current_as_locs[usize::from(swap.pos)]));
        }
    }

    (longest, longest_ends_in_top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn value(raw: u8) -> ValueNodeId {
        ValueNodeId::new(raw.into())
    }

    fn values(raw: &[u8]) -> Vec<ValueNodeId> {
        raw.iter().copied().map(value).collect()
    }

    const TEST_NODE_BUDGET: usize = 100_000;

    struct AssertPermutationBuilder {
        todo_preserve: Vec<u8>,
        last_uses: Vec<u8>,
        node_budget: usize,
        expect_ends_in_top: Option<bool>,
    }

    impl AssertPermutationBuilder {
        fn todo_preserve(mut self, values: impl AsRef<[u8]>) -> Self {
            self.todo_preserve = values.as_ref().into();
            self
        }

        fn last_uses(mut self, values: impl AsRef<[u8]>) -> Self {
            self.last_uses = values.as_ref().into();
            self
        }

        fn ends_in_top(mut self, ends: bool) -> Self {
            self.expect_ends_in_top = Some(ends);
            self
        }

        fn budget(mut self, budget: usize) -> Self {
            self.node_budget = budget;
            self
        }

        #[track_caller]
        fn assert(
            self,
            head_end: usize,
            current: impl AsRef<[u8]>,
            target: impl AsRef<[u8]>,
            expected: impl AsRef<[u16]>,
        ) {
            let todo_preserve = values(&self.todo_preserve);
            let current = values(current.as_ref());
            let target = values(target.as_ref());
            let last_uses = values(&self.last_uses);
            let (actual, ends_in_top) = permute_for_head(
                &todo_preserve,
                head_end,
                &current,
                &target,
                &last_uses,
                self.node_budget,
            );
            assert_eq!(actual, expected.as_ref());
            if let Some(expected_ends_in_top) = self.expect_ends_in_top {
                assert_eq!(expected_ends_in_top, ends_in_top);
            }
            let Some(replay) =
                replay_if_legal(&todo_preserve, head_end, &current, &target, &last_uses, &actual)
            else {
                panic!("replay not legal")
            };
            assert_eq!(ends_in_top, replay.stack[0] == target[target.len() - head_end])
        }
    }

    fn opts() -> AssertPermutationBuilder {
        AssertPermutationBuilder {
            todo_preserve: Vec::new(),
            last_uses: Vec::new(),
            node_budget: TEST_NODE_BUDGET,
            expect_ends_in_top: None,
        }
    }

    #[derive(Clone)]
    struct ReplayState {
        stack: Vec<ValueNodeId>,
        available: Vec<bool>,
        todo_preserve: Vec<ValueNodeId>,
    }

    impl ReplayState {
        fn new(current: &[ValueNodeId], todo_preserve: &[ValueNodeId]) -> Self {
            let mut available = vec![true; current.len()];
            available[0] = false;
            Self { stack: current.to_vec(), available, todo_preserve: todo_preserve.to_vec() }
        }

        fn try_swap(
            &self,
            depth: usize,
            head_end: usize,
            target: &[ValueNodeId],
            last_uses: &[ValueNodeId],
        ) -> Option<Self> {
            if depth == 0 || !self.available.get(depth).copied().unwrap_or(false) {
                return None;
            }

            let head_target = &target[target.len() - head_end..];
            let top = self.stack[0];
            let mut next = self.clone();
            if depth < head_end {
                if self.stack[depth] == head_target[depth] || top != head_target[depth] {
                    return None;
                }
            } else {
                let leaving_tail = self.stack[depth];
                if !last_uses.contains(&leaving_tail) || !head_target.contains(&leaving_tail) {
                    return None;
                }
                if let Some(preserve_idx) =
                    next.todo_preserve.iter().position(|&preserve| preserve == top)
                {
                    next.todo_preserve.swap_remove(preserve_idx);
                } else if target.contains(&top) {
                    return None;
                }
            }

            next.available[depth] = false;
            next.stack.swap(0, depth);
            Some(next)
        }
    }

    fn replay_if_legal(
        todo_preserve: &[ValueNodeId],
        head_end: usize,
        current: &[ValueNodeId],
        target: &[ValueNodeId],
        last_uses: &[ValueNodeId],
        executed: &[u16],
    ) -> Option<ReplayState> {
        let target_top = target[target.len() - head_end];
        let mut state = ReplayState::new(current, todo_preserve);
        for &depth in executed {
            if state.stack[0] == target_top {
                return None;
            }
            state = state.try_swap(usize::from(depth), head_end, target, last_uses)?;
        }
        Some(state)
    }

    #[test]
    fn correct_top_is_not_undone() {
        opts().ends_in_top(true).assert(3, [1, 3, 2], [1, 2, 3], []);
    }

    #[test]
    fn simple_cycle() {
        opts().ends_in_top(true).assert(2, [1, 2], [2, 1], [1]);
    }

    #[test]
    fn longer_cycle() {
        opts().assert(3, [1, 2, 3], [3, 1, 2], [1, 2]);
    }

    #[test]
    fn longest_branch_wins_over_fixing_top() {
        opts().ends_in_top(false).assert(4, [1, 2, 3, 4], [4, 1, 2, 1], [1, 2]);
    }

    #[test]
    fn returns_longest_partial_progress() {
        opts().assert(3, [1, 2, 3], [9, 1, 2], [1, 2]);
    }

    #[test]
    fn equal_length_branches_choose_first_position() {
        opts().assert(3, [1, 2, 3], [9, 1, 1], [1]);
    }

    #[test]
    fn irrelevant_head_value_moves_to_tail() {
        opts().last_uses([1]).assert(1, [9, 1], [1], [1]);
    }

    #[test]
    fn preservation_allows_head_value_to_move_to_tail() {
        opts().todo_preserve([2]).last_uses([1]).assert(1, [2, 1], [2, 1], [1]);
    }

    #[test]
    fn target_value_cannot_move_to_tail_without_preservation() {
        opts().last_uses([1]).assert(1, [2, 1], [2, 1], []);
    }

    #[test]
    fn preservation_is_consumed_once() {
        opts().todo_preserve([1]).last_uses([2, 3]).assert(
            3,
            [1, 1, 9, 2, 3],
            [1, 3, 2, 9],
            [3, 1],
        );
    }

    #[test]
    fn tail_value_cannot_leave_before_last_use() {
        opts().todo_preserve([2]).assert(1, [2, 1], [2, 1], []);
    }

    #[test]
    fn target_suffix_is_aligned_with_head() {
        opts().todo_preserve([7]).last_uses([1, 2]).assert(2, [7, 2, 1], [7, 1, 2], [2]);
    }

    #[test]
    fn node_budget_returns_best_expanded_state() {
        let last_uses = [1, 2, 3, 4];
        opts().budget(1).last_uses(last_uses).assert(4, [1, 2, 3, 4], [4, 1, 2, 1], []);
        opts().budget(2).last_uses(last_uses).assert(4, [1, 2, 3, 4], [4, 1, 2, 1], [1]);
        opts().budget(3).last_uses(last_uses).assert(4, [1, 2, 3, 4], [4, 1, 2, 1], [1, 2]);
    }

    #[test]
    #[should_panic(expected = "head must contain the stack top")]
    fn rejects_empty_head() {
        permute_for_head(&[], 0, &[value(1)], &[value(1)], &[], TEST_NODE_BUDGET);
    }

    #[test]
    #[should_panic(expected = "head exceeds current stack")]
    fn rejects_head_beyond_current() {
        permute_for_head(&[], 2, &[value(1)], &[value(1), value(2)], &[], TEST_NODE_BUDGET);
    }

    #[derive(Debug)]
    struct GeneratedContext {
        todo_preserve: Vec<ValueNodeId>,
        head_end: usize,
        current: Vec<ValueNodeId>,
        target: Vec<ValueNodeId>,
        last_uses: Vec<ValueNodeId>,
    }

    fn unique(mut values: Vec<u8>) -> Vec<u8> {
        values.sort_unstable();
        values.dedup();
        values
    }

    fn generated_context() -> impl Strategy<Value = GeneratedContext> {
        (prop::collection::vec(0u8..64, 1..65), prop::collection::vec(0u8..64, 1..65))
            .prop_flat_map(|(current, target)| {
                let max_head_end = current.len().min(target.len());
                (Just(current), Just(target), 1..=max_head_end)
            })
            .prop_flat_map(|(current, target, head_end)| {
                let last_use_candidates = unique(target.clone());
                let candidate_count = last_use_candidates.len();
                (
                    Just((current, target, head_end)),
                    prop::sample::subsequence(last_use_candidates, 0..=candidate_count),
                )
            })
            .prop_flat_map(|((current, target, head_end), last_uses)| {
                let preserve_candidates = unique(
                    current[..head_end]
                        .iter()
                        .copied()
                        .filter(|value| target.contains(value) && !last_uses.contains(value))
                        .collect(),
                );
                let candidate_count = preserve_candidates.len();
                (
                    Just((current, target, head_end, last_uses)),
                    prop::sample::subsequence(preserve_candidates, 0..=candidate_count),
                )
            })
            .prop_map(|((current, target, head_end, last_uses), todo_preserve)| GeneratedContext {
                todo_preserve: values(&todo_preserve),
                head_end,
                current: values(&current),
                target: values(&target),
                last_uses: values(&last_uses),
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn generated_swaps_are_legal_and_make_progress(
            context in generated_context(),
        ) {
            let GeneratedContext { todo_preserve, head_end, current, target, last_uses } = context;
            let (actual, ends_in_top) = permute_for_head(
                &todo_preserve,
                head_end,
                &current,
                &target,
                &last_uses,
                TEST_NODE_BUDGET,
            );
            let replayed = replay_if_legal(
                &todo_preserve,
                head_end,
                &current,
                &target,
                &last_uses,
                &actual,
            );

            prop_assert!(replayed.is_some());
            let replayed = replayed.unwrap();
            prop_assert_eq!(ends_in_top, replayed.stack[0] == target[target.len() - head_end]);
            let progressed_positions = replayed.available.iter().filter(|&&available| !available).count();
            prop_assert_eq!(progressed_positions, actual.len() + 1);

            let head_target = &target[target.len() - head_end..];
            for &depth in actual.iter() {
                let depth = depth as usize;
                if depth < head_end {
                    prop_assert_eq!(replayed.stack[depth], head_target[depth]);
                }
            }
        }
    }
}
