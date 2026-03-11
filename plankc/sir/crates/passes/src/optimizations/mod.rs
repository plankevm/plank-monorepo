mod constant_propagation;
mod copy_propagation;
mod defragmenter;
pub(crate) mod optimizer;
mod unused_operation_elimination;

pub use defragmenter::Defragmenter;
pub use optimizer::{Optimization, Optimizer, parse_passes_string, run_optimization};
