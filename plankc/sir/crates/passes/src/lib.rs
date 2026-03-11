pub mod analyses;
pub mod optimizations;
pub mod transforms;

pub use analyses::{
    AnalysesStore, AnalysisKind, BasicBlockOwnershipAndReachability, Cached,
    ControlFlowGraphInOutBundling, DefUse, DominanceFrontiers, Dominators, InOutGroupId,
    Predecessors, UseKind, UseLocation, legalize,
};
pub use optimizations::{Defragmenter, Optimizer, parse_passes_string};
pub use transforms::ssa_transform;
