use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct RepresentativeGraph {
    pub finalization: BlockFinalization,
    pub input_count: u32,
    pub operations: Box<[RepresentativeOperation]>,
    pub outputs_fifo: Box<[u32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockFinalization {
    ShuffleToOutputs,
    LastOpTerminates,
}

#[derive(Debug, Deserialize)]
pub struct RepresentativeOperation {
    pub inputs_fifo: Box<[u32]>,
    pub output_count: u32,
    pub effect_predecessors: Box<[u32]>,
    pub flippable: bool,
}

#[derive(Debug, Deserialize)]
pub struct RepresentativeSchedule(pub Box<[RepresentativeStackOp]>);

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepresentativeStackOp {
    Swap { depth: u8 },
    Dup { depth: u8 },
    Pop,
    Op { operation: u32 },
    Flipped { operation: u32 },
    Exchange { first_depth: u8, second_depth: u8 },
    Store { slot: u32 },
    Load { slot: u32 },
}
