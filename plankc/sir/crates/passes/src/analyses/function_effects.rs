use crate::{AnalysesStore, analyses::cache::Analysis};
use plank_core::{IndexVec, index_vec};
use sir_data::{BasicBlockId, EthIRProgram, FunctionId, operation::effects::Effect};

#[derive(Default)]
pub struct FunctionEffects {
    effects: IndexVec<FunctionId, Effect>,
}

impl FunctionEffects {
    pub fn effect_of(&self, fn_id: FunctionId) -> Effect {
        self.effects[fn_id]
    }
}

impl Analysis for FunctionEffects {
    fn compute(&mut self, program: &EthIRProgram, _store: &AnalysesStore) {
        self.effects.clear();
        self.effects.resize(program.functions.len(), Effect::PURE);

        let mut inner = FunctionEffectsAnalysis {
            effects: &mut self.effects,
            program,
            block_state: index_vec![BlockState::NotVisited; program.basic_blocks.len()],
        };

        inner.get_fn_effect(program.init_entry);
        if let Some(main_entry) = program.main_entry {
            inner.get_fn_effect(main_entry);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockState {
    NotVisited,
    Processing,
    Done,
}

struct FunctionEffectsAnalysis<'a> {
    effects: &'a mut IndexVec<FunctionId, Effect>,
    program: &'a EthIRProgram,
    block_state: IndexVec<BasicBlockId, BlockState>,
}

impl FunctionEffectsAnalysis<'_> {
    fn get_fn_effect(&mut self, fn_id: FunctionId) -> Effect {
        let entry_bb = self.program.function(fn_id).entry().id();
        match self.block_state[entry_bb] {
            BlockState::NotVisited => {
                let effect = self.aggregate_bb_effect(entry_bb).simplify();
                self.effects[fn_id] = effect;
                effect
            }
            BlockState::Processing => unreachable!("unexpected block state"),
            // Recursive functions are not allowed so if we're referencing a function and its
            // entry basic block was already processed then we know we've completed the
            // analysis of that function.
            BlockState::Done => self.effects[fn_id],
        }
    }

    fn aggregate_bb_effect(&mut self, bb_id: BasicBlockId) -> Effect {
        self.block_state[bb_id] = BlockState::Processing;

        let mut effect = Effect::PURE;
        let bb = self.program.block(bb_id);

        for op in bb.operations() {
            effect |= Effect::of(op.op()).unwrap_or_else(|callee| self.get_fn_effect(callee));
        }

        for succ in bb.successors() {
            match self.block_state[succ] {
                BlockState::NotVisited => effect |= self.aggregate_bb_effect(succ),
                BlockState::Processing => {
                    // We've detected a cycle, which may cause the function to out of gas error,
                    // other effects will be captured at the first insance.
                    effect |= Effect::REVERT;
                }
                BlockState::Done => {
                    // Block was already processed so its effect was already captured somewhere
                }
            }
        }

        self.block_state[bb_id] = BlockState::Done;

        effect
    }
}
