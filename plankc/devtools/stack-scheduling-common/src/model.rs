use serde::{Deserialize, Serialize};

pub const BLOCKS_FILE_NAME: &str = "blocks.csv";
pub const CANONICAL_BLOCKS_FILE_NAME: &str = "canonical-blocks.sqlite3";
pub const BLOCKS_HEADER: [&str; 3] = ["file", "block_id", "canonical_hash"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentativeGraph {
    pub finalization: BlockFinalization,
    pub input_count: u32,
    pub operations: Box<[RepresentativeOperation]>,
    pub outputs_fifo: Box<[u32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockFinalization {
    ShuffleToOutputs,
    LastOpTerminates,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentativeOperation {
    pub inputs_fifo: Box<[u32]>,
    pub output_count: u32,
    pub effect_predecessors: Box<[u32]>,
    pub flippable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepresentativeSchedule(pub Box<[RepresentativeStackOp]>);

impl RepresentativeSchedule {
    pub fn gas_cost(&self) -> u64 {
        self.0
            .iter()
            .map(|operation| match operation {
                RepresentativeStackOp::Swap { .. }
                | RepresentativeStackOp::Dup { .. }
                | RepresentativeStackOp::Pop => 3,
                RepresentativeStackOp::Exchange { .. } | RepresentativeStackOp::Store { .. } => 9,
                RepresentativeStackOp::Load { .. } => 6,
                RepresentativeStackOp::Op { .. } | RepresentativeStackOp::Flipped { .. } => 0,
            })
            .sum()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockRow {
    pub file: String,
    pub block_id: u32,
    pub canonical_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalBlockRow {
    pub canonical_hash: String,
    pub canonical_graph: String,
    pub best_schedule: String,
    pub best_gas_cost: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_and_schedule_json_round_trip() {
        let graph = RepresentativeGraph {
            finalization: BlockFinalization::ShuffleToOutputs,
            input_count: 2,
            operations: Box::new([RepresentativeOperation {
                inputs_fifo: Box::new([0, 1]),
                output_count: 1,
                effect_predecessors: Box::new([]),
                flippable: true,
            }]),
            outputs_fifo: Box::new([2]),
        };
        let schedule = RepresentativeSchedule(Box::new([
            RepresentativeStackOp::Swap { depth: 1 },
            RepresentativeStackOp::Flipped { operation: 0 },
        ]));

        let graph_text = serde_json::to_string(&graph).unwrap();
        let schedule_text = serde_json::to_string(&schedule).unwrap();

        assert_eq!(serde_json::from_str::<RepresentativeGraph>(&graph_text).unwrap(), graph);
        assert_eq!(
            serde_json::from_str::<RepresentativeSchedule>(&schedule_text).unwrap(),
            schedule
        );
        assert_eq!(
            graph_text,
            r#"{"finalization":"shuffle_to_outputs","input_count":2,"operations":[{"inputs_fifo":[0,1],"output_count":1,"effect_predecessors":[],"flippable":true}],"outputs_fifo":[2]}"#
        );
        assert_eq!(
            schedule_text,
            r#"[{"kind":"swap","depth":1},{"kind":"flipped","operation":0}]"#
        );
        assert_eq!(schedule.gas_cost(), 3);
    }
}
