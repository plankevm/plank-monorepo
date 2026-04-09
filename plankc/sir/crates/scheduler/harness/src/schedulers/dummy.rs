use crate::{
    intra_instr_scheduling::IntraInstrStrategy,
    types::{OperationGraph, Schedule, Scheduler, StackConfig},
};

pub struct DummyScheduler;

impl Scheduler for DummyScheduler {
    fn schedule(&self, _graph: &OperationGraph, config: &StackConfig) -> Schedule {
        let (input, output) = match config {
            StackConfig::Fixed { input, output } => (input, output),
            _ => panic!("DummyScheduler only supports Fixed config"),
        };
        let ops =
            IntraInstrStrategy::Greedy.solve(input, output, None).expect("greedy solve failed");
        Schedule::new(input.clone(), ops)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bench::{TestCase, run_test_case},
        types::{OperationGraphBuilder, ValueId},
    };

    #[test]
    fn empty_graph_swap() {
        let case = TestCase::new(
            "swap 3 values".into(),
            OperationGraphBuilder::new().build(),
            StackConfig::Fixed {
                input: vec![ValueId::new(1), ValueId::new(2), ValueId::new(3)],
                output: vec![ValueId::new(3), ValueId::new(1), ValueId::new(2)],
            },
        );
        let result = run_test_case(&DummyScheduler, &case).unwrap();
        assert_eq!(result.metrics().instruction_count(), 2);
        assert_eq!(result.metrics().gas_cost(), 6);
    }
}
