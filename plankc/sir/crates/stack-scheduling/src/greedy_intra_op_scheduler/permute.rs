use crate::op_graph::ValueNodeId;
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueLoc {
    Tail,
    Head(ValueNodeId),
}

impl ValueLoc {
    fn is_head_and_eq(self, value: ValueNodeId) -> bool {
        matches!(self, ValueLoc::Head(v) if v == value)
    }

    fn is_tail(self) -> bool {
        matches!(self, ValueLoc::Tail)
    }
}

#[derive(Clone, Copy)]
struct TargetHead<'a> {
    values: &'a [ValueNodeId],
}

impl TargetHead<'_> {
    fn contains(self, value: ValueNodeId) -> bool {
        self.values.contains(&value)
    }

    fn loc(self, i: u16) -> ValueLoc {
        self.values.get(usize::from(i)).copied().map_or(ValueLoc::Tail, ValueLoc::Head)
    }

    fn value(self, i: u16) -> ValueNodeId {
        self.values[usize::from(i)]
    }
}

fn expand_states(
    target: TargetHead<'_>,
    allow_swap_top: bool,
    best_so_far: &mut (usize, Vec<u16>),
    remaining_budget: &mut usize,
    current: Vec<ValueLoc>,
    to_preserve: &[ValueNodeId],
    executed: Vec<u16>,
    fixed: usize,
) -> Option<()> {
    let top = current[0];
    let can_preserve = match top {
        ValueLoc::Head(value) => to_preserve.contains(&value),
        ValueLoc::Tail => false,
    };
    let top_correct = top == target.loc(0);
    let mut end_of_path = true;

    for (pos, &have) in (0u16..).zip(&current).skip(1) {
        if top_correct && !allow_swap_top {
            break;
        }

        let want = target.loc(pos);
        if have == want {
            continue;
        }

        if top_correct {
            let has_at_least_one_destination =
                (0u16..).zip(&current).skip(1).any(|(dst_pos, &have_there)| {
                    let want_there = target.loc(dst_pos);
                    want_there == have && want_there != have_there
                });
            if !has_at_least_one_destination {
                continue;
            }
        } else {
            if !(top == want || (can_preserve && want.is_tail())) {
                continue;
            }
        }

        end_of_path = false;
        let mut fixed = fixed;
        if have == target.loc(0) {
            fixed += 1;
        }
        if top_correct {
            fixed -= 1;
        }
        if top == want || (can_preserve && want.is_tail()) {
            fixed += 1;
        }

        let mut current = current.to_vec();
        current[0] = have;
        current[usize::from(pos)] =
            if can_preserve && want.is_tail() { ValueLoc::Tail } else { top };
        let mut executed = executed.clone();
        executed.push(pos);

        if can_preserve && want.is_tail() {
            let to_preserve: Vec<_> =
                to_preserve.iter().copied().filter(|&v| !top.is_head_and_eq(v)).collect();
            expand_states(
                target,
                allow_swap_top,
                best_so_far,
                remaining_budget,
                current,
                &to_preserve,
                executed,
                fixed,
            )?;
        } else {
            expand_states(
                target,
                allow_swap_top,
                best_so_far,
                remaining_budget,
                current,
                to_preserve,
                executed,
                fixed,
            )?;
        }
    }

    if end_of_path {
        let new_is_better = match fixed.cmp(&best_so_far.0) {
            Ordering::Greater => true,
            Ordering::Equal => executed.len() < best_so_far.1.len(),
            Ordering::Less => false,
        };
        if new_is_better {
            *best_so_far = (fixed, executed);
        }
        *remaining_budget -= 1;
        if *remaining_budget == 0 {
            return None;
        }
    }

    Some(())
}

pub(crate) fn best_permute(
    to_preserve: &[ValueNodeId],
    last_uses: &[ValueNodeId],
    head_end: usize,
    current: &[ValueNodeId],
    full_target: &[ValueNodeId],
    paths_budget: usize,
    allow_swap_top: bool,
) -> Vec<u16> {
    assert!(head_end > 0, "head must contain the stack top");
    assert!(head_end <= current.len(), "head exceeds current stack");
    assert!(head_end <= full_target.len(), "head exceeds target stack");

    let target_depth_delta = full_target.len() - head_end;
    let (head, tail) = current.split_at(head_end);
    let target = TargetHead { values: &full_target[target_depth_delta..] };
    let mut current_as_locs = Vec::with_capacity(current.len());

    for (&current, &target_value) in head.iter().zip(target.values) {
        let mut loc = ValueLoc::Head(current);
        if current != target_value {
            if !full_target.contains(&current) {
                loc = ValueLoc::Tail;
            }
        }
        current_as_locs.push(loc);
    }
    for &current in tail {
        current_as_locs.push(if last_uses.contains(&current) {
            ValueLoc::Head(current)
        } else {
            ValueLoc::Tail
        });
    }

    let mut remaining_budget = paths_budget;
    let fixed_at_start = if target.loc(0) == current_as_locs[0] { 1 } else { 0 };
    let mut best = (fixed_at_start, Vec::new());

    if paths_budget > 0 {
        let _ = expand_states(
            target,
            allow_swap_top,
            &mut best,
            &mut remaining_budget,
            current_as_locs,
            to_preserve,
            Vec::new(),
            fixed_at_start,
        );
    }

    best.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashbrown::HashMap;
    use proptest::prelude::*;

    fn value(raw: u8) -> ValueNodeId {
        ValueNodeId::new(raw.into())
    }

    fn values(raw: &[u8]) -> Vec<ValueNodeId> {
        raw.iter().copied().map(value).collect()
    }

    const TEST_PATHS_BUDGET: usize = 100;

    fn build_to_preserve(
        head_end: usize,
        current: &[ValueNodeId],
        target: &[ValueNodeId],
        last_uses: &[ValueNodeId],
    ) -> Vec<ValueNodeId> {
        let tail = &current[head_end..];
        let mut to_preserve = target
            .iter()
            .copied()
            .filter(|v| !last_uses.contains(v) && !tail.contains(v))
            .collect::<Vec<_>>();
        to_preserve.sort();
        to_preserve.dedup();
        to_preserve
    }

    fn compute_head_end(
        current: &[ValueNodeId],
        target: &[ValueNodeId],
        last_uses: &[ValueNodeId],
    ) -> usize {
        let mut cur_counts = HashMap::<ValueNodeId, usize>::new();
        for &cur in current {
            *cur_counts.entry(cur).or_default() += 1;
        }
        let mut target_counts = HashMap::<ValueNodeId, usize>::new();
        for &t in target {
            *target_counts.entry(t).or_default() += 1;
        }
        let mut missing_copies = 0;
        for &v in target_counts.keys() {
            let cur = cur_counts.get(&v).copied().unwrap_or(0);
            let t = target_counts.get(&v).copied().unwrap_or(0);
            if last_uses.contains(&v) {
                missing_copies += t - cur;
            } else {
                missing_copies += (t + 1) - cur;
            }
        }
        target.len() - missing_copies
    }

    struct AssertPermutationBuilder {
        last_uses_override: Option<Vec<u8>>,
        paths_budget: usize,
        allow_swap_top: bool,
    }

    impl AssertPermutationBuilder {
        fn last_uses(mut self, values: impl AsRef<[u8]>) -> Self {
            self.last_uses_override = Some(values.as_ref().into());
            self
        }

        fn budget(mut self, budget: usize) -> Self {
            self.paths_budget = budget;
            self
        }

        fn allow_swap_top(mut self) -> Self {
            self.allow_swap_top = true;
            self
        }

        #[track_caller]
        fn assert(
            self,
            current: impl AsRef<[u8]>,
            target: impl AsRef<[u8]>,
            expected: impl AsRef<[u16]>,
        ) {
            let current = values(current.as_ref());
            let target = values(target.as_ref());
            let last_uses = match self.last_uses_override {
                Some(last_uses) => values(&last_uses),
                None => unique(target.clone()),
            };

            let head_end = compute_head_end(&current, &target, &last_uses);
            let to_preserve = build_to_preserve(head_end, &current, &target, &last_uses);

            let actual = best_permute(
                &to_preserve,
                &last_uses,
                head_end,
                &current,
                &target,
                self.paths_budget,
                self.allow_swap_top,
            );
            assert_eq!(actual, expected.as_ref(), "actual != expected (head_end = {head_end})");
            assert!(
                swap_sequence_legal(
                    head_end,
                    &current,
                    &target,
                    &last_uses,
                    &actual,
                    self.allow_swap_top
                ),
                "replay of {:?} not legal",
                actual
            );
        }
    }

    fn opts() -> AssertPermutationBuilder {
        AssertPermutationBuilder {
            last_uses_override: None,
            paths_budget: TEST_PATHS_BUDGET,
            allow_swap_top: false,
        }
    }

    fn swap_sequence_legal(
        head_end: usize,
        current: &[ValueNodeId],
        target: &[ValueNodeId],
        last_uses: &[ValueNodeId],
        executed: &[u16],
        allow_swap_top: bool,
    ) -> bool {
        {
            let mut cur_counts = HashMap::<ValueNodeId, u32>::new();
            for &cur in current {
                *cur_counts.entry(cur).or_default() += 1;
            }
            let mut target_counts = HashMap::<ValueNodeId, u32>::new();
            for &t in target {
                *target_counts.entry(t).or_default() += 1;
            }
            let mut missing_copies = 0u32;
            for &v in target_counts.keys() {
                let cur = cur_counts.get(&v).copied().unwrap_or(0);
                let t = target_counts.get(&v).copied().unwrap_or(0);
                if last_uses.contains(&v) {
                    missing_copies += t - cur;
                } else {
                    missing_copies += (t + 1) - cur;
                }
            }
            assert_eq!(head_end, target.len() - (missing_copies as usize));
        }

        let start_to_preserve = build_to_preserve(head_end, current, target, last_uses);
        let target_depth_delta = target.len() - head_end;

        let mut current = current.to_vec();
        let mut to_preserve = start_to_preserve.clone();
        let mut swapped_out_correct_top = false;

        for &swap in executed {
            let i = usize::from(swap);
            let top_want = target[target_depth_delta];
            let top = current[0];
            let have = current[i];

            if !allow_swap_top && top == top_want {
                eprintln!("swapping out correct top when allow_swap_top=false");
                return false;
            }

            if i < head_end {
                let want = target[target_depth_delta + i];
                if have == want {
                    eprintln!("swapping to already correct position {i}");
                    return false;
                }
                if top == have {
                    eprintln!("swapping identical values {top} at {i}");
                    return false;
                }

                if top == want {
                    swapped_out_correct_top = false;
                } else {
                    if swapped_out_correct_top {
                        eprintln!("swapped out correct top at end for no gain");
                        return false;
                    }
                    if top == top_want {
                        swapped_out_correct_top = true;
                    } else if have != top_want {
                        eprintln!("swapping without fixing target or top");
                        return false;
                    }
                }
            } else if target.contains(&top) {
                let Some(pi) = to_preserve.iter().position(|&v| v == top) else {
                    eprintln!("swapping {top} to tail despite not in `to_preserve`");
                    return false;
                };
                to_preserve.swap_remove(pi);
                if !last_uses.contains(&have) {
                    eprintln!("swapping {have} from tail but is not last use");
                    return false;
                }
            }

            current.swap(0, i);
        }

        if swapped_out_correct_top {
            eprintln!("swapped out correct top at end for no gain");
            return false;
        }

        true
    }

    #[test]
    fn correct_top_is_not_undone() {
        opts().assert([1, 3, 2], [1, 2, 3], []);
    }

    #[test]
    fn simple_cycle() {
        opts().last_uses([1, 2]).assert([1, 2], [2, 1], [1]);
    }

    #[test]
    fn longer_cycle() {
        opts().assert([1, 2, 3], [3, 1, 2], [1, 2]);
    }

    #[test]
    fn longest_branch_wins_over_fixing_top() {
        opts().last_uses([2, 4]).allow_swap_top().assert([1, 2, 3, 4, 1, 1], [4, 1, 2, 1], [1, 2]);
    }

    #[test]
    fn returns_longest_partial_progress() {
        opts().last_uses([1, 2]).assert([1, 2, 3, 9, 9], [9, 1, 2], [1, 2]);
    }

    #[test]
    fn equal_length_branches_choose_first_position() {
        opts().last_uses([]).assert([1, 2, 3, 1, 1, 9, 9], [9, 1, 1], [1]);
    }

    #[test]
    fn irrelevant_head_value_moves_to_tail() {
        opts().last_uses([1]).assert([9, 1], [1], [1]);
    }

    #[test]
    fn preservation_allows_head_value_to_move_to_tail() {
        opts().last_uses([1]).assert([2, 1], [2, 1], [1]);
    }

    #[test]
    fn preservation_is_consumed_once() {
        opts().last_uses([2, 3]).assert([1, 1, 9, 2, 3], [1, 3, 2, 9], [4]);
    }

    #[test]
    fn tail_progress_enables_head_progress() {
        opts().last_uses([1, 4]).assert([9, 1, 4, 2, 3], [3, 1, 2, 4], [2, 1]);
    }

    #[test]
    fn tail_value_cannot_leave_before_last_use() {
        opts().assert([2, 1], [2, 1], []);
    }

    #[test]
    fn target_suffix_is_aligned_with_head() {
        opts().last_uses([1, 2]).assert([7, 2, 1], [7, 1, 2], [2]);
    }

    #[test]
    fn paths_budget_returns_best_traced_path() {
        let current = [1, 8, 2, 3, 1, 1, 9, 9];
        let target = [9, 1, 1, 2];
        opts().last_uses([2]).budget(0).assert(current, target, []);
        opts().last_uses([2]).budget(1).assert(current, target, [1]);
        opts().last_uses([2]).budget(2).assert(current, target, [2, 3]);
    }

    #[test]
    fn allow_swap_top_fixes_remaining() {
        opts().last_uses([1, 2, 3, 4]).allow_swap_top().assert(
            [1, 3, 4, 2],
            [1, 2, 3, 4],
            [1, 2, 3, 1],
        );
    }

    #[derive(Debug)]
    struct GeneratedContext {
        current: Vec<ValueNodeId>,
        target: Vec<ValueNodeId>,
        last_uses: Vec<ValueNodeId>,
    }

    fn unique<T: Ord>(mut values: Vec<T>) -> Vec<T> {
        values.sort_unstable();
        values.dedup();
        values
    }

    fn generated_context() -> impl Strategy<Value = GeneratedContext> {
        prop::collection::vec(0u8..64, 1..33)
            .prop_flat_map(|target| {
                let target_len = target.len();
                (Just(target.clone()), prop::sample::subsequence(target, 1..=target_len))
            })
            .prop_flat_map(|(target, operand_copies)| {
                let last_use_candidates = unique(operand_copies.clone());
                let candidate_count = last_use_candidates.len();
                (
                    Just((target, operand_copies)),
                    prop::sample::subsequence(last_use_candidates, 0..=candidate_count),
                )
            })
            .prop_flat_map(|((target, operand_copies), last_uses)| {
                let mut current = operand_copies;
                current.extend(unique(
                    target.iter().copied().filter(|value| !last_uses.contains(value)).collect(),
                ));
                (Just((target, last_uses, current)), prop::collection::vec(64u8..128, 0..17))
            })
            .prop_flat_map(|((target, last_uses, mut current), irrelevant)| {
                current.extend(irrelevant);
                let current_len = current.len();
                (
                    Just((target, last_uses, current)),
                    prop::collection::vec(any::<u16>(), current_len),
                )
            })
            .prop_map(|((target, last_uses, current), ordering)| {
                let mut positions = (0..current.len()).collect::<Vec<_>>();
                positions.sort_by_key(|&position| ordering[position]);
                let current =
                    positions.into_iter().map(|position| current[position]).collect::<Vec<_>>();
                GeneratedContext {
                    current: values(&current),
                    target: values(&target),
                    last_uses: values(&last_uses),
                }
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn generated_swaps_are_legal(context in generated_context()) {
            let GeneratedContext { current, target, last_uses } = context;
            let head_end = compute_head_end(&current, &target, &last_uses);
            let to_preserve = build_to_preserve(head_end, &current, &target, &last_uses);
            let actual = best_permute(
                &to_preserve,
                &last_uses,
                head_end,
                &current,
                &target,
                TEST_PATHS_BUDGET,
                false
            );
            let replayed = swap_sequence_legal(
                head_end,
                &current,
                &target,
                &last_uses,
                &actual,
                false
            );

            prop_assert!(replayed);
        }
    }
}
