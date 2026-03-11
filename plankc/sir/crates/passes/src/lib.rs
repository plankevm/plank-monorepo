pub mod analyses;
pub mod optimizations;
pub mod transforms;

pub use analyses::{
    AnalysesStore, AnalysisKind, BasicBlockOwnershipAndReachability, ControlFlowGraphInOutBundling,
    DefUse, DominanceFrontiers, Dominators, InOutGroupId, Predecessors, UseKind, UseLocation,
    legalize,
};
pub use optimizations::{
    Defragmenter, Optimization, Optimizer, parse_passes_string, run_optimization,
};
pub use transforms::SsaTransform;
