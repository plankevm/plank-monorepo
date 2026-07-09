use hashbrown::{HashMap, hash_map::Entry};
use smallvec::SmallVec;

use crate::op_graph::ValueNodeId;

enum GoesToTail {
    Yes { preserve_idx: Option<usize> },
    No,
}

#[derive(Debug, PartialEq, Eq, Hash)]
enum ValueLoc {
    Value(ValueNodeId),
    Tail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ChainId(u16);

#[derive(Debug, Clone, Copy)]
struct Position {
    prev: Option<u16>,
    next: Option<u16>,
    part_of: Option<ChainId>,
}

#[derive(Debug, Clone, Copy)]
struct Chain {
    is_cycle: bool,
    start: u16,
}

struct StackView<'a> {
    last_uses: &'a [ValueNodeId],
    head: &'a [ValueNodeId],
    tail: &'a [ValueNodeId],
    target: &'a [ValueNodeId],
    unexpanded: &'a [ValueNodeId],
    head_target: &'a [ValueNodeId],
    todo_preserve: Vec<ValueNodeId>,

    chains: SmallVec<[Chain; 4]>,
    positions: Box<[Position]>,
    values: HashMap<ValueLoc, u16>,
}

impl StackView<'_> {
    fn chain(&self, id: ChainId) -> Chain {
        self.chains[id.0 as usize]
    }

    fn can_go_to_tail(&self, value: ValueNodeId) -> GoesToTail {
        if let Some(idx) = self.todo_preserve.iter().position(|&v| value == v) {
            return GoesToTail::Yes { preserve_idx: Some(idx) };
        }

        if !self.target.contains(&value) {
            return GoesToTail::Yes { preserve_idx: None };
        }

        GoesToTail::No
    }

    fn find_next_swap_index(&mut self, value: ValueNodeId) -> Option<u16> {
        let current_len = (self.head.len() + self.tail.len()) as u16;
        if let GoesToTail::Yes { preserve_idx } = self.can_go_to_tail(value) {
            for i in (self.head_len()..current_len).rev() {
                if self.swappable(i) {
                    self.remove_preserve(preserve_idx);
                    return Some(i);
                }
            }
        }

        for (i, &t) in (0..self.head_len()).zip(self.head_target) {
            if self.swappable(i) && t == value {
                return Some(i);
            }
        }

        None
    }

    fn may_leave_tail(&self, value: ValueNodeId) -> bool {
        self.last_uses.contains(&value) && self.head_target.contains(&value)
    }

    fn head_len(&self) -> u16 {
        self.head.len() as u16
    }

    fn swappable(&self, i: u16) -> bool {
        if !self.unswapped(i) {
            return false;
        }

        if let Some(&target) = self.head_target.get(i as usize) {
            target != self.head[i as usize]
        } else {
            self.may_leave_tail(self.tail[i as usize - self.head.len()])
        }
    }

    fn unswapped(&self, i: u16) -> bool {
        self.positions[i as usize].next.is_none()
    }

    fn remove_preserve(&mut self, idx: Option<usize>) {
        if let Some(idx) = idx {
            self.todo_preserve.swap_remove(idx);
        }
    }

    fn set_next(&mut self, i: u16, next: u16) {
        self.positions[i as usize].next = Some(next);
        self.positions[next as usize].prev = Some(i);
    }

    fn splice_cycles(&mut self, new_id: ChainId, i1: u16, other_id: ChainId, i2: u16) {
        assert!(self.chain(new_id).is_cycle);
        assert!(self.chain(other_id).is_cycle);
        assert!(new_id != other_id);
        assert!(i1 != i2);
        let next1 = self.positions[i1 as usize].next.expect("cycle member without next");
        let next2 = self.positions[i2 as usize].next.expect("cycle member without next");
        self.set_next(i1, next2);
        self.set_next(i2, next1);

        for pos in &mut self.positions {
            if let Some(part_of) = pos.part_of.as_mut() {
                if *part_of == other_id {
                    *part_of = new_id;
                }
            }
        }
    }

    fn map_chain(&mut self, is_cycle: bool, start: u16) {
        let id = ChainId(self.chains.len().try_into().unwrap());
        self.chains.push(Chain { is_cycle, start });

        let mut i = start;
        while let Some(next) = self.positions[i as usize].next
            && next != start
        {
            let loc = if next < self.head_len() {
                ValueLoc::Value(self.head[i as usize])
            } else {
                ValueLoc::Tail
            };

            self.positions[i as usize].part_of = Some(id);

            match self.values.entry(loc) {
                Entry::Vacant(vacant) => {
                    vacant.insert(i);
                }
                Entry::Occupied(mut occupied) => {
                    let &posi = occupied.get();
                    let pos = self.positions[posi as usize];
                    let other_id = pos.part_of.unwrap();

                    let other = self.chains[other_id.0 as usize];
                    if !other.is_cycle {
                        occupied.insert(i);
                    }

                    if is_cycle && other.is_cycle && id != other_id {
                        self.splice_cycles(id, i, other_id, posi);
                    }
                    if !is_cycle && other_id == id {
                        todo!("split degen")
                    }
                }
            }

            i = next;
        }
    }
}

pub(crate) fn permute_for_head(
    todo_preserve: &[ValueNodeId],
    head_end: usize,
    current: &[ValueNodeId],
    target: &[ValueNodeId],
    last_uses: &[ValueNodeId],
) {
    let current_len = u16::try_from(current.len()).expect("`current` not valid stack");
    let mut swap_chain: Box<[_]> = (0..current_len).collect();

    let mut view = {
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
            todo_preserve: todo_preserve.to_vec(),
            positions: vec![Position { prev: None, next: None, part_of: None }; current.len()]
                .into_boxed_slice(),
            chains: SmallVec::new(),
            merged_cycles_map: HashMap::with_capacity(4),
        }
    };

    'find_next_cycle: for start_chain_idx in 0..current_len {
        if !view.swappable(start_chain_idx) {
            continue;
        }

        let mut step = start_chain_idx;
        let is_cycle = 'map_chain: {
            'step: for _step_map_chain in 0..current_len {
                let value = current[step as usize];

                match view.find_next_swap_index(value) {
                    Some(i) => {
                        view.set_next(step, i);
                        if i == start_chain_idx {
                            break 'map_chain true;
                        } else {
                            step = i;
                            continue 'step;
                        }
                    }
                    None => break 'map_chain false,
                }
            }

            unreachable!("infinite loop / stuck");
        };
    }
}
