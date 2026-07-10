use crate::op_graph::ValueNodeId;

#[derive(Clone, Copy)]
enum ValueLoc {
    Tail,
    Head(ValueNodeId),
}

impl ValueLoc {
    fn is_head_and_eq(self, value: ValueNodeId) -> bool {
        match self {
            ValueLoc::Head(v) => v == value,
            ValueLoc::Tail => false,
        }
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

pub(crate) fn permute_for_head(
    todo_preserve: &[ValueNodeId],
    head_end: usize,
    current: &[ValueNodeId],
    target: &[ValueNodeId],
    last_uses: &[ValueNodeId],
) -> Vec<u16> {
    let mut swaps = Vec::with_capacity(head_end);
    let target_depth_delta = target.len() - head_end;
    let (head, tail) = current.split_at(head_end);
    let head_target = &target[target_depth_delta..];
    let current_len = u16::try_from(current.len()).expect("overflow");

    let mut current_as_locs = Vec::with_capacity(current.len());
    for ((pos, &current), &tv) in (0..head_end as u16).zip(head).zip(head_target) {
        let mut loc = ValueLoc::Head(current);
        if current != tv {
            swaps.push(Swap { accepts: ValueLoc::Head(tv), pos });
            if !target.contains(&current) {
                loc = ValueLoc::Tail;
            }
        }
        current_as_locs.push(loc);
    }
    for (pos, &current) in (head_end as u16..current_len).zip(tail) {
        current_as_locs.push(ValueLoc::Head(current));
        if last_uses.contains(&current) && head_target.contains(&current) {
            swaps.push(Swap { accepts: ValueLoc::Tail, pos });
        }
    }

    let mut start_state = State { swaps, todo_preserve: todo_preserve.to_vec(), executed: vec![] };
    let loc = current_as_locs[0];

    if loc.is_head_and_eq(head_target[0]) {
        return vec![];
    }
}
