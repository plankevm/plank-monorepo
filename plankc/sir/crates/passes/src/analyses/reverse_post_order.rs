use crate::analyses::{AnalysesStore, cache::Analysis};
use plank_core::{DenseIndexMap, DenseIndexSet, dense_index_map::Entry};
use sir_data::{BasicBlockId, EthIRProgram, FunctionId, Operation};

#[derive(Debug, Clone, Default)]
pub struct ReversePostOrder {
    visited_blocks: DenseIndexSet<BasicBlockId>,
    function_blocks_rpo: DenseIndexMap<FunctionId, Vec<BasicBlockId>>,
    functions_rpo: Vec<FunctionId>,
}

impl Analysis for ReversePostOrder {
    fn compute(&mut self, program: &EthIRProgram, _store: &AnalysesStore) {
        self.visited_blocks.clear();
        self.function_blocks_rpo.clear();
        self.functions_rpo.clear();
        self.functions_rpo.reserve_exact(program.functions.len());

        self.visit_function(program, program.init_entry);
        if let Some(main_entry) = program.main_entry {
            self.visit_function(program, main_entry);
        }
        self.functions_rpo.reverse();
    }
}

impl ReversePostOrder {
    fn visit_function(&mut self, program: &EthIRProgram, function: FunctionId) {
        let Entry::Vacant(entry) = self.function_blocks_rpo.entry(function) else {
            return;
        };

        entry.insert(Vec::new());
        self.visit_block(program, function, program.functions[function].entry());
        self.function_blocks_rpo
            .get_mut(function)
            .expect("visited function should have an associated block postorder")
            .reverse();
        self.functions_rpo.push(function);
    }

    fn visit_block(&mut self, program: &EthIRProgram, function: FunctionId, block: BasicBlockId) {
        if !self.visited_blocks.add(block) {
            return;
        }

        for operation in program.basic_blocks[block].operations.iter() {
            if let Operation::InternalCall(call) = program.operations[operation] {
                self.visit_function(program, call.function);
            }
        }
        for successor in program.block(block).successors() {
            self.visit_block(program, function, successor);
        }
        self.function_blocks_rpo
            .get_mut(function)
            .expect("visited function should have an associated block postorder")
            .push(block);
    }

    pub fn blocks_rpo(&self) -> impl Iterator<Item = &BasicBlockId> {
        self.functions_rpo.iter().flat_map(|&function| self.function_blocks_rpo[function].iter())
    }

    pub fn function_blocks_rpo(&self, function: FunctionId) -> Option<&[BasicBlockId]> {
        self.function_blocks_rpo.get(function).map(Vec::as_slice)
    }

    pub fn blocks_postorder(&self) -> impl Iterator<Item = &BasicBlockId> {
        self.functions_rpo
            .iter()
            .rev()
            .flat_map(|&function| self.function_blocks_rpo[function].iter().rev())
    }

    pub fn functions_rpo(&self) -> &[FunctionId] {
        &self.functions_rpo
    }

    pub fn functions_postorder(&self) -> impl Iterator<Item = &FunctionId> {
        self.functions_rpo.iter().rev()
    }
}
