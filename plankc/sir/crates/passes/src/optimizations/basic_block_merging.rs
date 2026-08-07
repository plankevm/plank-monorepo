use crate::Pass;

pub struct BasicBlockMerger {}

impl Pass for BasicBlockMerger {
    fn run(&mut self, program: &mut sir_data::EthIRProgram, _store: &crate::AnalysesStore) {
        todo!()
    }
}
