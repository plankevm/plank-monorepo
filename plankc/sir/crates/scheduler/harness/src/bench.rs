// Runs a scheduler over test cases and collects metrics + timing.

use std::time::{Duration, Instant};

use crate::{
    types::{OperationGraph, Scheduler, StackConfig},
    verifier::{Metrics, Verifier, VerifierError},
};

pub struct TestCase {
    name: String,
    operation_graph: OperationGraph,
    stack_config: StackConfig,
}

impl TestCase {
    pub fn new(name: String, operation_graph: OperationGraph, stack_config: StackConfig) -> Self {
        Self { name, operation_graph, stack_config }
    }
}

pub struct BenchmarkResult {
    name: String,
    metrics: Metrics,
    solve_time: Duration,
}

impl BenchmarkResult {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn solve_time(&self) -> Duration {
        self.solve_time
    }
}

pub fn run_test_case(
    scheduler: &dyn Scheduler,
    case: &TestCase,
) -> Result<BenchmarkResult, VerifierError> {
    let start = Instant::now();
    let schedule = scheduler.schedule(&case.operation_graph, &case.stack_config);
    let solve_time = start.elapsed();

    let mut verifier = Verifier::new(&case.operation_graph, &case.stack_config, &schedule);
    let metrics = verifier.verify()?;

    Ok(BenchmarkResult { name: case.name.clone(), metrics, solve_time })
}

pub fn run_test_cases<'a>(
    scheduler: &'a dyn Scheduler,
    cases: &'a [TestCase],
) -> impl Iterator<Item = Result<BenchmarkResult, VerifierError>> + 'a {
    cases.iter().map(move |case| run_test_case(scheduler, case))
}
