use hashbrown::HashMap;

use crate::types::{ScheduledOp, ValueId};

use super::{IntraInstrError, Stack, count_occurrences};

pub(super) fn solve(
    current: &[ValueId],
    target: &[ValueId],
    spilled: Option<&HashMap<u32, ValueId>>,
) -> Result<Vec<ScheduledOp>, IntraInstrError> {
    let mut stack = Stack::new(current);
    let mut target_counts = count_occurrences(target);
    pop_unneeded(&mut stack, &mut target_counts.clone())?;
    fill_target(&mut stack, target, &mut target_counts, spilled)?;
    Ok(stack.into_ops())
}

fn pop_unneeded(
    stack: &mut Stack,
    target_counts: &mut HashMap<ValueId, usize>,
) -> Result<(), IntraInstrError> {
    // Scan bottom-to-top - we prioritize popping values closer to the top.
    let mut needed = vec![true; stack.len()];
    for i in 0..stack.len() {
        match target_counts.get_mut(&stack.get(i)) {
            Some(count) if *count > 0 => *count -= 1,
            _ => needed[i] = false,
        }
    }
    // Swap unneeded values to top and pop, working top-down.
    for i in (0..stack.len()).rev() {
        if needed[i] {
            continue;
        }
        let pos = stack.top() - i;
        if pos > 0 {
            stack.swap(pos as u8)?;
        }
        stack.pop();
    }
    Ok(())
}

fn fill_target(
    stack: &mut Stack,
    target: &[ValueId],
    target_counts: &mut HashMap<ValueId, usize>,
    spilled: Option<&HashMap<u32, ValueId>>,
) -> Result<(), IntraInstrError> {
    let mut stack_counts = stack.count_occurrences();

    for pos in 0..target.len() {
        let val = target[pos];
        *target_counts.entry(val).or_default() -= 1;

        if pos < stack.len() && stack.get(pos) == val {
            *stack_counts.entry(val).or_default() -= 1;
            if stack_counts.get(&val).copied().unwrap_or(0) == 0
                && target_counts.get(&val).copied().unwrap_or(0) > 0
            {
                stack.dup((stack.top() - pos + 1) as u8)?;
                *stack_counts.entry(val).or_default() += 1;
            }
            continue;
        }

        let found = (pos + 1..stack.len()).find(|&idx| stack.get(idx) == val);

        if let Some(found_at) = found {
            let remaining_on_stack = stack_counts.get(&val).copied().unwrap_or(0);
            let still_needed = target_counts.get(&val).copied().unwrap_or(0);

            if remaining_on_stack > still_needed {
                if found_at != stack.top() {
                    stack.swap((stack.top() - found_at) as u8)?;
                }
                stack.swap((stack.top() - pos) as u8)?;
                *stack_counts.entry(val).or_default() -= 1;
            } else {
                stack.dup((stack.top() - found_at + 1) as u8)?;
                stack.swap((stack.top() - pos) as u8)?;
            }
        } else if let Some((&offset, _)) = spilled.and_then(|s| s.iter().find(|(_, v)| **v == val))
        {
            stack.load(val, offset);
            stack.swap((stack.top() - pos) as u8)?;
        } else {
            return Err(IntraInstrError::ValueUnavailable(val));
        }
    }

    Ok(())
}
