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
    fn loc(self, i: u16) -> ValueLoc {
        self.values.get(usize::from(i)).copied().map_or(ValueLoc::Tail, ValueLoc::Head)
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
    let mut progress_made = false;

    for (pos, &have) in (0u16..).zip(&current).skip(1) {
        if top_correct && !allow_swap_top {
            break;
        }

        let want = target.loc(pos);
        if have == want || have == top {
            continue;
        }

        if top_correct {
            let can_preserve_this = match have {
                ValueLoc::Tail => false,
                ValueLoc::Head(value) => to_preserve.contains(&value),
            };

            let has_at_least_one_destination =
                (0u16..).zip(&current).skip(1).any(|(dst_pos, &have_there)| {
                    let want_there = target.loc(dst_pos);
                    let valid_there =
                        want_there == have || (want_there == ValueLoc::Tail && can_preserve_this);
                    let already_correct = want_there == have_there;
                    valid_there && !already_correct
                });
            if !has_at_least_one_destination {
                continue;
            }
        } else {
            if !(top == want || (can_preserve && want.is_tail())) {
                continue;
            }
        }

        progress_made = true;
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

    if !progress_made && !top_correct {
        let top_want = target.loc(0);
        assert!(matches!(top_want, ValueLoc::Head(_)));
        for ((pos, &have), &want) in (0u16..).zip(&current).zip(target.values).skip(1) {
            if have.is_head_and_eq(want) || have != top_want {
                continue;
            }

            let mut current = current.to_vec();
            current.swap(0, usize::from(pos));
            let mut executed = executed.clone();
            executed.push(pos);

            expand_states(
                target,
                allow_swap_top,
                best_so_far,
                remaining_budget,
                current,
                to_preserve,
                executed,
                fixed + 1,
            )?;

            progress_made = true;
        }
    }

    if !progress_made {
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
        if current != target_value && !full_target.contains(&current) {
            loc = ValueLoc::Tail;
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
            if last_uses.contains(&v) || cur == 0 {
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

            let tail = &current[head_end..];
            for (i, &a) in tail.iter().enumerate() {
                if !target.contains(&a) {
                    continue;
                }
                for &b in &tail[i + 1..] {
                    assert_ne!(
                        a, b,
                        "invalid inputs: target value {a} present more than once in tail (head_end = {head_end})"
                    );
                }
            }

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
            if let Err(msg) = swap_sequence_legal(
                head_end,
                &current,
                &target,
                &last_uses,
                &to_preserve,
                &actual,
                self.allow_swap_top,
                self.paths_budget,
            ) {
                panic!("replay of {:?} not legal (head_end={head_end}): {msg}", actual);
            }
            assert_eq!(actual, expected.as_ref(), "actual != expected (head_end = {head_end})");
        }
    }

    fn opts() -> AssertPermutationBuilder {
        AssertPermutationBuilder {
            last_uses_override: None,
            paths_budget: TEST_PATHS_BUDGET,
            allow_swap_top: false,
        }
    }

    fn progress_to_be_made(
        head_end: usize,
        current: &[ValueNodeId],
        target: &[ValueNodeId],
        last_uses: &[ValueNodeId],
        swap_until_completion: bool,
    ) -> bool {
        if head_end == 0 {
            return false;
        }

        let target_depth_delta = target.len() - head_end;
        if !swap_until_completion {
            // Conservatively say no progress to be made instead of reimplementing permute logic.
            return false;
        }
        let tail = &current[head_end..];

        for (&cur, &tgt) in current.iter().zip(&target[target_depth_delta..]) {
            if cur != tgt {
                if current
                    .iter()
                    .zip(&target[target_depth_delta..])
                    .any(|(&ocur, &otgt)| ocur != otgt && ocur == tgt)
                {
                    return true;
                }

                if tail.iter().any(|&value| value == tgt && last_uses.contains(&value)) {
                    return true;
                }
            }
        }

        false
    }

    fn swap_sequence_legal(
        head_end: usize,
        current: &[ValueNodeId],
        target: &[ValueNodeId],
        last_uses: &[ValueNodeId],
        to_preserve: &[ValueNodeId],
        executed: &[u16],
        allow_swap_top: bool,
        budget: usize,
    ) -> Result<(), String> {
        let target_depth_delta = target.len() - head_end;

        let mut current = current.to_vec();
        let mut to_preserve = to_preserve.to_vec();

        if executed.is_empty()
            && budget > 0
            && progress_to_be_made(head_end, &current, target, last_uses, allow_swap_top)
        {
            return Err("returned empty sequence when can make progress".into());
        }

        for &swap in executed {
            let i = usize::from(swap);
            let top_want = target[target_depth_delta];
            let top = current[0];
            let have = current[i];

            if !allow_swap_top && top == top_want {
                return Err("swapping out correct top when allow_swap_top=false".into());
            }

            if i < head_end {
                let want = target[target_depth_delta + i];
                if have == want {
                    return Err("swapping to already correct position {i}".into());
                }
                if top == have {
                    return Err("swapping identical values {top} at {i}".into());
                }

                if top != want && top != top_want && have != top_want {
                    return Err("swapping without fixing target or top".into());
                }
            } else if target.contains(&top) {
                if let Some(pi) = to_preserve.iter().position(|&v| v == top) {
                    to_preserve.swap_remove(pi);
                } else if allow_swap_top && top == top_want {
                    if !last_uses.contains(&have) {
                        return Err("swapping {have} from tail but is not last use".into());
                    }
                } else {
                    return Err("swapping {top} to tail despite not in `to_preserve`".into());
                }
            }

            current.swap(0, i);
        }

        Ok(())
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
    fn low_budget_returns_suboptimal_path() {
        opts().last_uses([0, 1]).budget(1).assert([2, 0, 1], [0, 1, 2], [1]);
        opts().last_uses([0, 1]).budget(2).assert([2, 0, 1], [0, 1, 2], [2]);
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
    fn allow_swap_top_fixes_remaining() {
        opts().last_uses([1, 2, 3, 4]).allow_swap_top().assert(
            [1, 3, 4, 2],
            [1, 2, 3, 4],
            [1, 2, 3, 1],
        );
    }

    #[test]
    fn replace_top_with_tail_for_progress() {
        opts().last_uses([0, 5]).allow_swap_top().assert(
            [5, 1, 0, 1, 2, 3, 4, 5, 6],
            [5, 1, 0, 5],
            [3, 7],
        );
    }

    fn unique<T: Ord>(mut values: Vec<T>) -> Vec<T> {
        values.sort_unstable();
        values.dedup();
        values
    }

    fn generated_context()
    -> impl Strategy<Value = (Vec<ValueNodeId>, bool, Vec<ValueNodeId>, Vec<ValueNodeId>)> {
        (
            prop::collection::vec(any::<(u8, u16)>(), 1..32),
            prop::collection::vec(any::<(u8, u16)>(), 0..32),
            prop::collection::vec(any::<(u8, u16)>(), 0..32),
        )
            .prop_flat_map(|(stack_target, spilled_target, unused)| {
                let start = {
                    let mut start = [stack_target.clone(), unused].concat();
                    start.sort_by_key(|&(id, _)| id);
                    start.dedup_by_key(|(id, _)| *id);
                    start.sort_by_key(|&(_, w)| w);
                    start.into_iter().map(|(id, _)| id).collect::<Vec<_>>()
                };

                let target = {
                    let mut target = [stack_target.clone(), spilled_target].concat();
                    target.sort_by_key(|&(_, w)| w);
                    target.into_iter().map(|(id, _)| id).collect::<Vec<_>>()
                };

                let possible_last_use =
                    unique(stack_target.iter().copied().map(|(id, _)| id).collect());
                let total_possible_last_use = possible_last_use.len();

                (
                    Just((target, start)),
                    prop::sample::subsequence(possible_last_use, 1..=total_possible_last_use),
                )
            })
            .prop_flat_map(|((target, start), last_uses)| {
                let (target, start, last_uses) = {
                    let mut old_new_map = [None; 256];
                    let mut next_new_id = 0;
                    let mut remap = |ids: Vec<u8>| {
                        ids.into_iter()
                            .map(|id| {
                                if let Some(new_id) = old_new_map[id as usize] {
                                    new_id
                                } else {
                                    let new_id = next_new_id;
                                    next_new_id += 1;
                                    old_new_map[id as usize] = Some(new_id);
                                    new_id
                                }
                            })
                            .collect::<Vec<_>>()
                    };
                    (remap(target), remap(start), remap(last_uses))
                };

                let head_end = last_uses.len();
                let mut to_push = target.clone();
                for &last_use in &last_uses {
                    let position = to_push
                        .iter()
                        .position(|&value| value == last_use)
                        .expect("last use must be in target");
                    to_push.remove(position);
                }
                let min_pushes = usize::from(head_end == 0);
                let max_pushes = to_push.len();

                (
                    Just((target, start, last_uses, head_end)),
                    prop::sample::subsequence(to_push, min_pushes..=max_pushes).prop_shuffle(),
                )
            })
            .prop_flat_map(|((target, start, last_uses, start_head_end), to_push)| {
                let swaps = (0..to_push.len())
                    .map(|offset| 0..=start_head_end + offset)
                    .collect::<Vec<_>>();
                let final_head_end = start_head_end + to_push.len();
                let max_allow_swap_top = u8::from(final_head_end == target.len());

                (
                    Just((target, start, last_uses, start_head_end, to_push)),
                    swaps,
                    0..=max_allow_swap_top,
                )
            })
            .prop_map(
                |(
                    (target, mut current, last_uses, mut head_end, to_push),
                    swaps,
                    allow_swap_top,
                )| {
                    for (value, swap) in to_push.into_iter().zip(swaps) {
                        if !current.contains(&value) {
                            current.insert(head_end, value);
                        }
                        current.insert(0, value);
                        head_end += 1;
                        current.swap(0, swap);
                    }

                    (values(&last_uses), allow_swap_top != 0, values(&current), values(&target))
                },
            )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(512))]

        #[test]
        fn generated_swaps_are_legal(
            (last_uses, allow_swap_top, current, target) in generated_context(),
        ) {
            let head_end = compute_head_end(&current, &target, &last_uses);
            let to_preserve = build_to_preserve(head_end, &current, &target, &last_uses);

            let actual = best_permute(
                &to_preserve,
                &last_uses,
                head_end,
                &current,
                &target,
                TEST_PATHS_BUDGET,
                allow_swap_top
            );
            let replayed = swap_sequence_legal(
                head_end,
                &current,
                &target,
                &last_uses,
                &to_preserve,
                &actual,
                allow_swap_top,
                TEST_PATHS_BUDGET
            );

            if let Err(msg) = replayed {
                prop_assert!(false, "{msg}: actual = {actual:?}, head_end = {head_end}, to_preserve = {to_preserve:?}" )
            }
        }
    }
}
