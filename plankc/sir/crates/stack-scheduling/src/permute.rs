use hashbrown::HashMap;
use plank_core::{DenseIndexMap, dense_index_map::Entry};

use crate::op_graph::ValueNodeId;

#[derive(Debug, Clone, Copy)]
struct Connect {
    last_from: u16,
    cycle_start: u16,
}

fn cycle_get_remapped(map: &mut HashMap<u16, u16>, cycle_id: u16) -> u16 {
    let mut remapped = cycle_id;
    let mut depth = 0;
    while let Some(&actual) = map.get(&remapped) {
        remapped = actual;
        depth += 1;
    }
    if depth > 1 {
        map.insert(cycle_id, remapped);
    }
    remapped
}

/// Returns the index to swap the current's value
/// to for every position in the stack. May contain entries where `moves[i] = i` which means no swap
/// is required to/from that position.
///
/// # Panics
/// Panics if `current` is not a permutation of `target`.
pub fn permute_cycles(current: &[ValueNodeId], target: &[ValueNodeId]) -> (u16, Box<[u16]>) {
    assert_eq!(current.len(), target.len(), "permute requires equal lengths");
    let len = u16::try_from(current.len()).expect("too large for stack");

    let mut moves: Box<_> = (0..len).collect();
    let mut last: DenseIndexMap<ValueNodeId, Connect> = DenseIndexMap::new();
    let mut cycle_id_map = HashMap::with_capacity(4);
    let mut total_cycles = 0u16;

    'find_next_cycle: for cycle_start in 0..len {
        if moves[cycle_start as usize] != cycle_start {
            continue;
        }
        total_cycles += 1;
        if current[cycle_start as usize] == target[cycle_start as usize] {
            continue;
        }

        let mut step = cycle_start;

        for _ in 0..len {
            let value = current[step as usize];
            let Some(i) = (0..len).zip(target).rev().find_map(|(i, &t)| {
                ((i == moves[i as usize] || i == cycle_start)
                    && current[i as usize] != t
                    && t == value)
                    .then_some(i)
            }) else {
                panic!("not a permutation: i={}; {:?} {:?} {:?}", step, current, target, moves)
            };

            match last.entry(value) {
                Entry::Vacant(vacant) => {
                    vacant.insert(Connect { last_from: step, cycle_start });
                    moves[step as usize] = i;
                }
                Entry::Occupied(entry) => {
                    let entry_cycle_start =
                        cycle_get_remapped(&mut cycle_id_map, entry.cycle_start);
                    if entry_cycle_start == cycle_start {
                        moves[step as usize] = i;
                    } else {
                        // Same value is part of another cycle, so we splice them together to
                        // create the larger cycle.
                        total_cycles -= 1;
                        let other_from = entry.last_from;
                        let other_to = moves[other_from as usize];
                        moves[other_from as usize] = i;
                        moves[step as usize] = other_to;
                        *entry = Connect { last_from: step, cycle_start };
                        // Remap cycle id so we don't accidentally split the same cycle
                        // later.
                        cycle_id_map.insert(entry_cycle_start, cycle_start);
                    }
                }
            }

            step = i;

            if step == cycle_start {
                continue 'find_next_cycle;
            }
        }

        unreachable!("infinite loop / stuck");
    }

    (total_cycles, moves)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_permute(
        current: impl AsRef<[u32]>,
        target: impl AsRef<[u32]>,
        expected_moves: impl AsRef<[u16]>,
    ) {
        assert_permute_and_cycles(current, target, expected_moves, 1);
    }

    fn assert_permute_and_cycles(
        current: impl AsRef<[u32]>,
        target: impl AsRef<[u32]>,
        expected_moves: impl AsRef<[u16]>,
        expected_total_cycles: u16,
    ) {
        let current: Vec<_> = current.as_ref().iter().copied().map(ValueNodeId::new).collect();
        let target: Vec<_> = target.as_ref().iter().copied().map(ValueNodeId::new).collect();
        let (cycle_count, moves) = permute_cycles(&current, &target);

        assert_eq!(moves.as_ref(), expected_moves.as_ref(), "moves, actual != expected");
        assert_eq!(cycle_count, expected_total_cycles, "cycle count, actual != expected");
    }

    #[test]
    fn single_cycle() {
        assert_permute([2, 3, 1], [1, 2, 3], [1, 2, 0]);
    }

    #[test]
    fn separate_cycles() {
        assert_permute_and_cycles([2, 3, 1, 5, 4], [1, 2, 3, 4, 5], [1, 2, 0, 4, 3], 2);
    }

    #[test]
    fn separate_but_together_1() {
        assert_permute([1, 1, 2, 3], [2, 3, 1, 1], [3, 2, 0, 1]);
    }

    #[test]
    fn separate_but_together_2() {
        assert_permute([1, 1, 2, 3], [3, 2, 1, 1], [2, 3, 1, 0]);
    }

    #[test]
    fn two_rights_make_a_wrong() {
        assert_permute([1, 1, 2, 4, 3, 2], [3, 4, 1, 2, 2, 1], [2, 5, 3, 1, 0, 4]);
    }
}
