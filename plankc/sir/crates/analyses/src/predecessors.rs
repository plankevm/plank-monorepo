use sir_data::{BasicBlockId, EthIRProgram, IndexVec};

pub struct Predecessors {
    pub(crate) inner: IndexVec<BasicBlockId, Vec<BasicBlockId>>,
}

impl Predecessors {
    pub fn new() -> Self {
        Self { inner: IndexVec::new() }
    }
}

impl std::ops::Index<BasicBlockId> for Predecessors {
    type Output = Vec<BasicBlockId>;
    fn index(&self, id: BasicBlockId) -> &Vec<BasicBlockId> {
        &self.inner[id]
    }
}

impl std::ops::IndexMut<BasicBlockId> for Predecessors {
    fn index_mut(&mut self, id: BasicBlockId) -> &mut Vec<BasicBlockId> {
        &mut self.inner[id]
    }
}

impl Predecessors {
    pub fn compute(&mut self, program: &EthIRProgram) {
        for pred in self.inner.iter_mut() {
            pred.clear();
        }
        self.inner.resize(program.basic_blocks.len(), Vec::new());

        for block in program.blocks() {
            for successor in block.successors() {
                self.inner[successor].push(block.id());
            }
        }
    }
}

pub fn compute_predecessors(
    program: &EthIRProgram,
    predecessors: &mut IndexVec<BasicBlockId, Vec<BasicBlockId>>,
) {
    for pred in predecessors.iter_mut() {
        pred.clear();
    }
    predecessors.resize(program.basic_blocks.len(), Vec::new());

    for block in program.blocks() {
        for successor in block.successors() {
            predecessors[successor].push(block.id());
        }
    }
}
