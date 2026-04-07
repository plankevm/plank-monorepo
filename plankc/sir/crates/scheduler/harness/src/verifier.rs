// Validates scheduler output: dependency correctness, ordering constraints, completeness.

use crate::types::{OperationGraph, Schedule, StackConfig};

pub struct Metrics {
    instruction_count: u32,
    gas_cost: u32,
}

pub enum VerifierError {}

pub struct Verifier<'a> {
    operation_graph: &'a OperationGraph,
    stack_config: &'a StackConfig,
    schedule: &'a Schedule,
}

impl<'a> Verifier<'a> {
    pub fn new(
        operation_graph: &'a OperationGraph,
        stack_config: &'a StackConfig,
        schedule: &'a Schedule,
    ) -> Self {
        Self { operation_graph, stack_config, schedule }
    }

    pub fn verify(&self) -> Result<Metrics, VerifierError> {
        todo!()
    }
}
