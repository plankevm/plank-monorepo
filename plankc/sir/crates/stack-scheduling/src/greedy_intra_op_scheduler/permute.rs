use smallvec::SmallVec;

use crate::op_graph::ValueNodeId;

struct StackView<'a> {
    last_uses: &'a [ValueNodeId],
    head: &'a [ValueNodeId],
    tail: &'a [ValueNodeId],
    target: &'a [ValueNodeId],
    unexpanded: &'a [ValueNodeId],
    head_target: &'a [ValueNodeId],
    swap_chain: &'a mut [u16],
    todo_preserve: Vec<ValueNodeId>,
}

enum GoesToTail {
    Yes { preserve_idx: Option<usize> },
    No,
}

impl StackView<'_> {
    fn can_go_to_tail(&self, value: ValueNodeId) -> GoesToTail {
        if let Some(idx) = self.todo_preserve.iter().position(|&v| value == v) {
            return GoesToTail::Yes { preserve_idx: Some(idx) };
        }

        if !self.target.contains(&value) {
            return GoesToTail::Yes { preserve_idx: None };
        }

        GoesToTail::No
    }

    fn may_leave_tail(&self, value: ValueNodeId) -> bool {
        self.last_uses.contains(&value) && self.head_target.contains(&value)
    }

    fn unswapped(&self, i: u16) -> bool {
        self.swap_chain[i as usize] == i
    }

    fn remove_preserve(&mut self, idx: Option<usize>) {
        if let Some(idx) = idx {
            self.todo_preserve.swap_remove(idx);
        }
    }
}

pub(crate) fn permute_for_head(
    todo_preserve: &[ValueNodeId],
    head_end: usize,
    current: &[ValueNodeId],
    target: &[ValueNodeId],
    last_uses: &[ValueNodeId],
    swap_chain: &mut [u16],
) {
    for (c, i) in swap_chain.iter_mut().zip(0..) {
        *c = i;
    }

    let view = {
        let target_depth_delta = target.len() - head_end;
        let (head, tail) = current.split_at(head_end);
        let (unexpanded, head_target) = target.split_at(target_depth_delta);
        StackView {
            last_uses,
            head,
            tail,
            target,
            unexpanded,
            head_target,
            swap_chain,
            todo_preserve: todo_preserve.to_vec(),
        }
    };

    let mut candidates = SmallVec::<[(u16, CandidateKind); 32]>::new();
    let mut ci = 0u16;

    'swap_next: for _ in 0..current.len() + 1 {
        candidates.clear();

        let value = current[ci as usize];
        if value == view.head_target[0] {
            return;
        }

        if let GoesToTail::Yes { preserve_idx } = view.can_go_to_tail(value) {
            for (i, &tv) in (head_end..head_end + view.tail.len()).zip(view.tail).rev() {
                let i = u16::try_from(i).unwrap();
                if view.may_leave_tail(tv) && view.unswapped(i) {
                    swap_chain[ci as usize] = i;
                    ci = i;
                    view.remove_preserve(preserve_idx);
                    continue 'swap_next;
                }
            }
        }

        if !view.head_target.contains(&value) {
            // Unexpanded value.
            return;
        }

        for (i, (&target, &current)) in head_target.iter().zip(head).enumerate().rev() {
            let i = u16::try_from(i).unwrap();
            if target == value && target != current && swap_chain[i as usize] == i {}
        }
    }

    unreachable!("didn't naturally terminate");
}

enum CandidateKind {
    Top,
    Unexpanded,
    CanGoToTail,
    Head,
}

