use std::collections::BTreeSet;

use alloy_primitives::U256;

use crate::InstructionId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolicAtom {
    FunctionInput(u16),
    Constant { instruction: InstructionId, value: U256 },
    InstructionResult { instruction: InstructionId, output: u8 },
    CallResult { call: InstructionId, output: u16 },
}

pub type SymbolicValue = BTreeSet<SymbolicAtom>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolicStack {
    values: Vec<SymbolicValue>,
    next_input: u16,
    required_inputs: u16,
    open: bool,
}

impl SymbolicStack {
    pub fn open() -> Self {
        Self { values: Vec::new(), next_input: 0, required_inputs: 0, open: true }
    }

    pub fn empty() -> Self {
        Self { values: Vec::new(), next_input: 0, required_inputs: 0, open: false }
    }

    pub fn values_top_first(&self) -> &[SymbolicValue] {
        &self.values
    }

    pub const fn next_input(&self) -> u16 {
        self.next_input
    }

    pub const fn required_inputs(&self) -> u16 {
        self.required_inputs
    }

    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn push_atom(&mut self, atom: SymbolicAtom) {
        self.values.insert(0, BTreeSet::from([atom]));
    }

    pub fn push_value(&mut self, value: SymbolicValue) {
        self.values.insert(0, value);
    }

    pub fn pop(&mut self) -> Result<SymbolicValue, StackUnderflow> {
        self.materialize(1)?;
        Ok(self.values.remove(0))
    }

    pub fn materialize(&mut self, depth: usize) -> Result<(), StackUnderflow> {
        while self.values.len() < depth {
            if !self.open {
                return Err(StackUnderflow);
            }
            let input = self.next_input;
            self.next_input = self.next_input.checked_add(1).ok_or(StackUnderflow)?;
            self.required_inputs = self.required_inputs.max(self.next_input);
            self.values
                .insert(self.values.len(), BTreeSet::from([SymbolicAtom::FunctionInput(input)]));
        }
        Ok(())
    }

    pub fn duplicate(&mut self, depth: u8) -> Result<(), StackUnderflow> {
        self.materialize(depth as usize)?;
        self.values.insert(0, self.values[depth as usize - 1].clone());
        Ok(())
    }

    pub fn swap(&mut self, depth: u8) -> Result<(), StackUnderflow> {
        self.materialize(depth as usize + 1)?;
        self.values.swap(0, depth as usize);
        Ok(())
    }

    pub fn normalize(&mut self) {
        while self.open && self.next_input > 0 {
            let expected = SymbolicAtom::FunctionInput(self.next_input - 1);
            if self.values.last().is_none_or(|value| value.len() != 1 || !value.contains(&expected))
            {
                break;
            }
            self.values.pop();
            self.next_input -= 1;
        }
    }

    pub fn merge(&mut self, other: &Self) -> Result<bool, StackShapeMismatch> {
        let mut left = self.clone();
        let mut right = other.clone();
        left.normalize();
        right.normalize();
        if left.open != right.open
            || left.next_input != right.next_input
            || left.values.len() != right.values.len()
        {
            return Err(StackShapeMismatch {
                left_values: left.values.len(),
                left_next_input: left.next_input,
                right_values: right.values.len(),
                right_next_input: right.next_input,
            });
        }
        let mut changed = false;
        let required_inputs = left.required_inputs.max(right.required_inputs);
        changed |= required_inputs != left.required_inputs;
        left.required_inputs = required_inputs;
        for (left_value, right_value) in left.values.iter_mut().zip(&right.values) {
            let old_len = left_value.len();
            left_value.extend(right_value);
            changed |= left_value.len() != old_len;
        }
        *self = left;
        Ok(changed)
    }

    pub fn finite_values(
        &self,
        input_count: u16,
    ) -> Result<Vec<SymbolicValue>, StackShapeMismatch> {
        let mut stack = self.clone();
        if stack.next_input > input_count || stack.required_inputs > input_count {
            return Err(StackShapeMismatch {
                left_values: stack.values.len(),
                left_next_input: stack.next_input,
                right_values: input_count as usize,
                right_next_input: input_count,
            });
        }
        while stack.next_input < input_count {
            stack.materialize(stack.values.len() + 1).expect("open stack should materialize");
        }
        Ok(stack.values)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("stack underflow")]
pub struct StackUnderflow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "stack shapes differ: left explicit={left_values}, next input={left_next_input}; right explicit={right_values}, next input={right_next_input}"
)]
pub struct StackShapeMismatch {
    pub left_values: usize,
    pub left_next_input: u16,
    pub right_values: usize,
    pub right_next_input: u16,
}
