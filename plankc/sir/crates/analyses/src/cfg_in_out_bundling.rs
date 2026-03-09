use plank_core::{newtype_index, span::IncIterable};
use sir_data::{BasicBlockId, EthIRProgram, IndexVec};

newtype_index! {
    pub struct InOutGroupId;
}

#[derive(Debug)]
pub struct ControlFlowGraphInOutBundling {
    out_group: IndexVec<BasicBlockId, Option<InOutGroupId>>,
    in_group: IndexVec<BasicBlockId, Option<InOutGroupId>>,
    next_group_id: InOutGroupId,
}

impl Default for ControlFlowGraphInOutBundling {
    fn default() -> Self {
        Self::new()
    }
}

impl ControlFlowGraphInOutBundling {
    pub fn new() -> Self {
        Self {
            out_group: IndexVec::new(),
            in_group: IndexVec::new(),
            next_group_id: InOutGroupId::default(),
        }
    }

    pub fn compute(&mut self, program: &EthIRProgram) {
        for g in self.out_group.iter_mut() {
            *g = None;
        }
        for g in self.in_group.iter_mut() {
            *g = None;
        }
        self.out_group.resize(program.basic_blocks.len(), None);
        self.in_group.resize(program.basic_blocks.len(), None);
        self.next_group_id = InOutGroupId::default();

        for block in program.blocks() {
            let existing_group_id = block.successors().find_map(|to| self.in_group[to]);
            let group_id = existing_group_id.unwrap_or_else(|| self.next_group_id.get_and_inc());
            self.out_group[block.id()] = Some(group_id);
            for to in block.successors() {
                self.in_group[to] = Some(group_id);
            }
        }
    }

    pub fn analyze(ir: &EthIRProgram) -> Self {
        let mut result = Self::new();
        result.compute(ir);
        result
    }

    pub fn get_out_group(&self, bb_id: BasicBlockId) -> Option<InOutGroupId> {
        self.out_group[bb_id]
    }

    pub fn get_in_group(&self, bb_id: BasicBlockId) -> Option<InOutGroupId> {
        self.in_group[bb_id]
    }

    pub fn next_group_id(&self) -> InOutGroupId {
        self.next_group_id
    }
}
