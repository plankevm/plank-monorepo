// Built-in test cases covering various graph patterns and stack config variants.
//
// TODO: add a helper that converts a SIR basic block into an OperationGraph + StackConfig,
// automatically deriving ordering constraints from operation semantics (e.g. storage read/write
// ordering).

use crate::{
    bench::TestCase,
    types::{OperationGraphBuilder, StackConfig, ValueId},
};

pub fn entry_block_simple() -> TestCase {
    let mut builder = OperationGraphBuilder::new();
    let op0 = builder.add_op(vec![], vec![ValueId::new(1)]);
    let op1 = builder.add_op(vec![], vec![ValueId::new(2)]);
    let op2 = builder.add_op(vec![ValueId::new(1), ValueId::new(2)], vec![ValueId::new(3)]);
    builder.must_precede(op0, op2);
    builder.must_precede(op1, op2);

    TestCase::new("entry_block_simple".into(), builder.build(), StackConfig::FixedInput(vec![]))
}
