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

pub struct BenchmarkResult {
    name: String,
    result: Result<Metrics, VerifierError>,
    solve_time: Duration,
}

pub fn run_test_cases<'a>(
    scheduler: &'a dyn Scheduler,
    cases: &'a [TestCase],
) -> impl Iterator<Item = BenchmarkResult> + 'a {
    cases.iter().map(move |case| {
        let start = Instant::now();
        let schedule = scheduler.schedule(&case.operation_graph, &case.stack_config);
        let solve_time = start.elapsed();

        let verifier = Verifier::new(&case.operation_graph, &case.stack_config, &schedule);
        let verify_result = verifier.verify();

        BenchmarkResult { name: case.name.clone(), result: verify_result, solve_time }
    })
}
