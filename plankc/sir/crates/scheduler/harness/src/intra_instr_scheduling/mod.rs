mod greedy;

use hashbrown::HashMap;

use crate::types::{ScheduledOp, ValueId};

#[derive(Debug)]
pub enum IntraInstrError {
    ValueUnavailable(ValueId),
    StackDepthExceeded(u8),
}

macro_rules! define_strategies {
    ($($variant:ident),+ $(,)?) => {
        pub enum IntraInstrStrategy {
            $($variant),+
        }

        impl IntraInstrStrategy {
            pub const fn all() -> &'static [Self] {
                &[$(Self::$variant),+]
            }
        }
    }
}

define_strategies!(Greedy);

impl IntraInstrStrategy {
    pub fn solve(
        &self,
        current: &[ValueId],
        target: &[ValueId],
        spilled: Option<&HashMap<u32, ValueId>>,
    ) -> Result<Vec<ScheduledOp>, IntraInstrError> {
        match self {
            Self::Greedy => greedy::solve(current, target, spilled),
        }
    }
}

pub(crate) struct Stack {
    values: Vec<ValueId>,
    ops: Vec<ScheduledOp>,
}

impl Stack {
    pub(crate) fn new(values: &[ValueId]) -> Self {
        Self { values: values.to_vec(), ops: Vec::new() }
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn get(&self, pos: usize) -> ValueId {
        self.values[pos]
    }

    pub(crate) fn top(&self) -> usize {
        self.values.len() - 1
    }

    pub(crate) fn count_occurrences(&self) -> HashMap<ValueId, usize> {
        count_occurrences(&self.values)
    }

    pub(crate) fn swap(&mut self, pos: u8) -> Result<(), IntraInstrError> {
        if pos > 16 {
            return Err(IntraInstrError::StackDepthExceeded(pos));
        }
        let top = self.values.len() - 1;
        self.values.swap(top, top - pos as usize);
        self.ops.push(ScheduledOp::Swap(pos));
        Ok(())
    }

    pub(crate) fn dup(&mut self, pos: u8) -> Result<(), IntraInstrError> {
        if pos > 16 {
            return Err(IntraInstrError::StackDepthExceeded(pos));
        }
        let val = self.values[self.values.len() - pos as usize];
        self.values.push(val);
        self.ops.push(ScheduledOp::Dup(pos));
        Ok(())
    }

    pub(crate) fn pop(&mut self) {
        self.values.pop();
        self.ops.push(ScheduledOp::Pop);
    }

    pub(crate) fn load(&mut self, val: ValueId, offset: u32) {
        self.values.push(val);
        self.ops.push(ScheduledOp::Load { val, offset });
    }

    pub(crate) fn into_ops(self) -> Vec<ScheduledOp> {
        self.ops
    }
}

pub(crate) fn count_occurrences(values: &[ValueId]) -> HashMap<ValueId, usize> {
    let mut counts: HashMap<ValueId, usize> = HashMap::new();
    for val in values {
        *counts.entry(*val).or_default() += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRATEGIES: &[IntraInstrStrategy] = IntraInstrStrategy::all();

    fn val(id: u32) -> ValueId {
        ValueId::new(id)
    }

    fn replay(current: &[ValueId], ops: &[ScheduledOp]) -> Vec<ValueId> {
        let mut stack = current.to_vec();
        for op in ops {
            match op {
                ScheduledOp::Swap(pos) => {
                    let top = stack.len() - 1;
                    stack.swap(top, top - *pos as usize);
                }
                ScheduledOp::Dup(pos) => {
                    let val = stack[stack.len() - *pos as usize];
                    stack.push(val);
                }
                ScheduledOp::Pop => {
                    stack.pop();
                }
                ScheduledOp::Load { val, .. } => {
                    stack.push(*val);
                }
                _ => panic!("unexpected op in intra-instr test"),
            }
        }
        stack
    }

    #[test]
    fn identity() {
        let current = &[val(1), val(2), val(3)];
        let target = &[val(1), val(2), val(3)];
        for strategy in STRATEGIES {
            let ops = strategy.solve(current, target, None).unwrap();
            assert!(ops.is_empty());
        }
    }

    #[test]
    fn swap() {
        let current = &[val(1), val(2), val(3), val(4), val(5)];
        let target = &[val(1), val(4), val(3), val(2), val(5)];
        for strategy in STRATEGIES {
            let ops = strategy.solve(current, target, None).unwrap();
            assert_eq!(replay(current, &ops), target);
        }
    }

    #[test]
    fn pop_unneeded() {
        let current = &[val(1), val(2), val(3), val(4), val(5), val(3), val(6)];
        let target = &[val(3), val(1), val(2)];
        for strategy in STRATEGIES {
            let ops = strategy.solve(current, target, None).unwrap();
            assert_eq!(replay(current, &ops), target);
        }
    }

    #[test]
    fn dup() {
        let current = &[val(1), val(2), val(3)];
        let target = &[val(1), val(3), val(3), val(2)];
        for strategy in STRATEGIES {
            let ops = strategy.solve(current, target, None).unwrap();
            assert_eq!(replay(current, &ops), target);
        }
    }

    #[test]
    fn load_from_spill() {
        let current = &[val(1), val(2)];
        let target = &[val(3), val(1), val(2)];
        let mut spilled = HashMap::new();
        spilled.insert(0, val(3));
        for strategy in STRATEGIES {
            let ops = strategy.solve(current, target, Some(&spilled)).unwrap();
            assert_eq!(replay(current, &ops), target);
        }
    }

    #[test]
    fn value_unavailable() {
        let current = &[val(1), val(2)];
        let target = &[val(1), val(3)];
        for strategy in STRATEGIES {
            let result = strategy.solve(current, target, None);
            assert!(matches!(result, Err(IntraInstrError::ValueUnavailable(v)) if v == val(3)));
        }
    }

    #[test]
    fn multiple_dups() {
        let current = &[val(1), val(2), val(3)];
        let target = &[val(1), val(3), val(1), val(2), val(1)];
        for strategy in STRATEGIES {
            let ops = strategy.solve(current, target, None).unwrap();
            assert_eq!(replay(current, &ops), target);
        }
    }

    #[test]
    fn pop_all() {
        let current = &[val(1), val(2), val(3)];
        for strategy in STRATEGIES {
            let ops = strategy.solve(current, &[], None).unwrap();
            assert_eq!(replay(current, &ops), &[]);
        }
    }

    #[test]
    fn pop_swap_dup_load() {
        let current = &[val(1), val(2), val(3), val(4)];
        let target = &[val(3), val(5), val(2), val(2)];
        let mut spilled = HashMap::new();
        spilled.insert(0, val(5));
        for strategy in STRATEGIES {
            let ops = strategy.solve(current, target, Some(&spilled)).unwrap();
            assert_eq!(replay(current, &ops), target);
        }
    }

    #[test]
    fn stack_too_deep() {
        let current: Vec<ValueId> = (1..=18).map(val).collect();
        let mut target = current.clone();
        target.swap(0, 17);
        for strategy in STRATEGIES {
            let result = strategy.solve(&current, &target, None);
            assert!(matches!(result, Err(IntraInstrError::StackDepthExceeded(17))));
        }
    }
}
