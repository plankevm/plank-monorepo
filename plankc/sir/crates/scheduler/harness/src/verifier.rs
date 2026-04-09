// Validates scheduler output: dependency correctness, ordering constraints, completeness.

use hashbrown::HashMap;
use plank_core::DenseIndexSet;

use crate::types::{
    OpNodeIdx, OperationGraph, Schedule, ScheduleIdx, ScheduledOp, StackConfig, ValueId,
};

pub struct Metrics {
    instruction_count: u32,
    gas_cost: u32,
}

impl Metrics {
    pub fn instruction_count(&self) -> u32 {
        self.instruction_count
    }

    pub fn gas_cost(&self) -> u32 {
        self.gas_cost
    }
}

#[derive(Debug)]
pub enum VerifierError {
    InputStackMismatch { expected: Vec<ValueId>, actual: Vec<ValueId> },
    OutputStackMismatch { expected: Vec<ValueId>, actual: Vec<ValueId> },
    StackUnderflow(ScheduleIdx),
    OpInputMismatch { idx: ScheduleIdx, op: OpNodeIdx, expected: ValueId, actual: ValueId },
    OrderingViolation { idx: ScheduleIdx, op: OpNodeIdx, predecessor: OpNodeIdx },
    SpillMismatch { idx: ScheduleIdx, expected: ValueId, actual: ValueId },
    LoadMismatch { idx: ScheduleIdx, offset: u32, expected: ValueId, actual: Option<ValueId> },
    UnscheduledOps(Vec<OpNodeIdx>),
}

pub struct Verifier<'a> {
    operation_graph: &'a OperationGraph,
    stack_config: &'a StackConfig,
    schedule: &'a Schedule,
    stack: Vec<ValueId>,
    executed: DenseIndexSet<OpNodeIdx>,
    memory: HashMap<u32, ValueId>,
    max_memory_expansion: u32,
    instruction_count: u32,
    gas_cost: u32,
}

impl<'a> Verifier<'a> {
    pub fn new(
        operation_graph: &'a OperationGraph,
        stack_config: &'a StackConfig,
        schedule: &'a Schedule,
    ) -> Self {
        Self {
            operation_graph,
            stack_config,
            schedule,
            stack: Vec::new(),
            executed: DenseIndexSet::new(),
            memory: HashMap::new(),
            max_memory_expansion: 0,
            instruction_count: 0,
            gas_cost: 0,
        }
    }

    fn init_stack(&mut self) -> Result<(), VerifierError> {
        let starting_stack = self.schedule.starting_stack();
        match self.stack_config {
            StackConfig::FixedInput(expected) | StackConfig::Fixed { input: expected, .. } => {
                if starting_stack != expected {
                    return Err(VerifierError::InputStackMismatch {
                        expected: expected.clone(),
                        actual: starting_stack.to_vec(),
                    });
                }
            }
            StackConfig::Flexible | StackConfig::FixedOutput(_) | StackConfig::Matching => {}
        }
        self.stack.extend_from_slice(starting_stack);
        Ok(())
    }

    fn check_min_stack_depth(
        &self,
        required: usize,
        idx: ScheduleIdx,
    ) -> Result<(), VerifierError> {
        if self.stack.len() < required {
            return Err(VerifierError::StackUnderflow(idx));
        }
        Ok(())
    }

    fn handle_op(&mut self, op_idx: &OpNodeIdx, idx: ScheduleIdx) -> Result<(), VerifierError> {
        if let Some(predecessors) = self.operation_graph.must_precede(*op_idx) {
            for &pred in predecessors {
                if !self.executed.contains(pred) {
                    return Err(VerifierError::OrderingViolation {
                        idx,
                        op: *op_idx,
                        predecessor: pred,
                    });
                }
            }
        }

        let inputs = self.operation_graph.inputs(*op_idx);
        self.check_min_stack_depth(inputs.len(), idx)?;
        let inputs_start = self.stack.len() - inputs.len();
        let mut actual = self.stack[inputs_start..].to_vec();
        let mut expected = inputs.to_vec();
        if self.operation_graph.commutative(*op_idx) {
            actual.sort();
            expected.sort();
        }
        if let Some((expected, actual)) = expected.iter().zip(actual.iter()).find(|(e, a)| e != a) {
            return Err(VerifierError::OpInputMismatch {
                idx,
                op: *op_idx,
                expected: *expected,
                actual: *actual,
            });
        }
        self.stack.truncate(inputs_start);

        for &output in self.operation_graph.outputs(*op_idx) {
            self.stack.push(output);
        }

        self.executed.add(*op_idx);
        Ok(())
    }

    fn handle_swap(&mut self, pos: &u8, idx: ScheduleIdx) -> Result<(), VerifierError> {
        self.check_min_stack_depth(*pos as usize + 1, idx)?;
        let top = self.stack.len() - 1;
        self.stack.swap(top, top - *pos as usize);
        self.instruction_count += 1;
        self.gas_cost += 3;
        Ok(())
    }

    fn handle_dup(&mut self, pos: &u8, idx: ScheduleIdx) -> Result<(), VerifierError> {
        self.check_min_stack_depth(*pos as usize, idx)?;
        let val = self.stack[self.stack.len() - *pos as usize];
        self.stack.push(val);
        self.instruction_count += 1;
        self.gas_cost += 3;
        Ok(())
    }

    fn handle_pop(&mut self, idx: ScheduleIdx) -> Result<(), VerifierError> {
        self.check_min_stack_depth(1, idx)?;
        self.stack.pop();
        self.instruction_count += 1;
        self.gas_cost += 2;
        Ok(())
    }

    fn expand_memory(&mut self, offset: u32) {
        let memory_reach = offset + 32;
        if memory_reach > self.max_memory_expansion {
            let word_cost = |bytes: u32| -> u32 {
                let w = bytes / 32;
                w * 3 + w * w / 512
            };
            self.gas_cost += word_cost(memory_reach) - word_cost(self.max_memory_expansion);
            self.max_memory_expansion = memory_reach;
        }
    }

    fn handle_spill(
        &mut self,
        val: &ValueId,
        offset: &u32,
        idx: ScheduleIdx,
    ) -> Result<(), VerifierError> {
        self.check_min_stack_depth(1, idx)?;
        let actual = self.stack.pop().unwrap();
        if actual != *val {
            return Err(VerifierError::SpillMismatch { idx, expected: *val, actual });
        }
        self.memory.insert(*offset, *val);
        self.expand_memory(*offset);
        self.instruction_count += 1;
        self.gas_cost += 3;
        Ok(())
    }

    fn handle_load(
        &mut self,
        val: &ValueId,
        offset: &u32,
        idx: ScheduleIdx,
    ) -> Result<(), VerifierError> {
        let actual = self.memory.get(offset).copied();
        if actual != Some(*val) {
            return Err(VerifierError::LoadMismatch {
                idx,
                offset: *offset,
                expected: *val,
                actual,
            });
        }
        self.stack.push(*val);
        self.instruction_count += 1;
        self.gas_cost += 3;
        Ok(())
    }

    fn check_no_unscheduled_ops(&self) -> Result<(), VerifierError> {
        if self.executed.len() != self.operation_graph.op_count() {
            return Err(VerifierError::UnscheduledOps(
                self.operation_graph
                    .op_indices()
                    .filter(|idx| !self.executed.contains(*idx))
                    .collect(),
            ));
        }
        Ok(())
    }

    fn check_output_stack(&self) -> Result<(), VerifierError> {
        let expected = match self.stack_config {
            StackConfig::FixedOutput(expected) | StackConfig::Fixed { output: expected, .. } => {
                expected
            }
            StackConfig::Matching => self.schedule.starting_stack(),
            StackConfig::Flexible | StackConfig::FixedInput(_) => return Ok(()),
        };
        if self.stack != expected {
            return Err(VerifierError::OutputStackMismatch {
                expected: expected.to_vec(),
                actual: self.stack.clone(),
            });
        }
        Ok(())
    }

    fn simulate(&mut self) -> Result<(), VerifierError> {
        for (idx, op) in self.schedule.scheduled_ops() {
            match op {
                ScheduledOp::Op(op_idx) => self.handle_op(op_idx, idx)?,
                ScheduledOp::Swap(pos) => self.handle_swap(pos, idx)?,
                ScheduledOp::Dup(pos) => self.handle_dup(pos, idx)?,
                ScheduledOp::Pop => self.handle_pop(idx)?,
                ScheduledOp::Spill { val, offset } => self.handle_spill(val, offset, idx)?,
                ScheduledOp::Load { val, offset } => self.handle_load(val, offset, idx)?,
            }
        }
        Ok(())
    }

    pub fn verify(&mut self) -> Result<Metrics, VerifierError> {
        self.init_stack()?;
        self.simulate()?;
        self.check_no_unscheduled_ops()?;
        self.check_output_stack()?;
        Ok(Metrics { instruction_count: self.instruction_count, gas_cost: self.gas_cost })
    }
}
