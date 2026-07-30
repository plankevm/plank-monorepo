use plank_core::{DenseIndexSet, IndexVec, index_vec};
use sir_data::{BasicBlockId, EthIRProgram, FunctionId, Operation, OperationIdx};

use crate::{AnalysesStore, Pass};

#[derive(Debug, Clone, Default)]
pub(crate) struct Inliner {
    postorder: Vec<FunctionId>,
    callsites: IndexVec<FunctionId, Vec<CallSite>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CallSite {
    block: BasicBlockId,
    operation: OperationIdx,
}

impl Inliner {
    pub(crate) fn new(program: &EthIRProgram) -> Self {
        let mut callsites = index_vec![Vec::new(); program.functions.len()];
        let mut postorder = Vec::new();
        let mut function_states = index_vec![FunctionState::NotStarted; program.functions.len()];

        walk_call_graph(
            program,
            program.init_entry,
            &mut function_states,
            &mut postorder,
            &mut callsites,
        );
        if let Some(main_entry) = program.main_entry {
            walk_call_graph(
                program,
                main_entry,
                &mut function_states,
                &mut postorder,
                &mut callsites,
            );
        }

        Self { postorder, callsites }
    }
}

impl Pass for Inliner {
    fn run(&mut self, _program: &mut EthIRProgram, _store: &AnalysesStore) {
        todo!("implement inlining pass")
    }
}

fn walk_call_graph(
    program: &EthIRProgram,
    function_id: FunctionId,
    function_states: &mut IndexVec<FunctionId, FunctionState>,
    postorder: &mut Vec<FunctionId>,
    callsites: &mut IndexVec<FunctionId, Vec<CallSite>>,
) {
    match function_states[function_id] {
        FunctionState::Complete => return,
        FunctionState::InProgress => {
            unreachable!("recursive calls should be rejected before inlining")
        }
        FunctionState::NotStarted => {}
    }

    function_states[function_id] = FunctionState::InProgress;

    let mut visited_blocks = DenseIndexSet::new();
    let mut worklist = vec![program.functions[function_id].entry()];

    while let Some(block) = worklist.pop() {
        if !visited_blocks.add(block) {
            continue;
        }

        for op_id in program.basic_blocks[block].operations.iter() {
            if let Operation::InternalCall(data) = program.operations[op_id] {
                callsites[data.function].push(CallSite { block, operation: op_id });
                walk_call_graph(program, data.function, function_states, postorder, callsites);
            }
        }

        worklist.extend(program.basic_blocks[block].control.iter_outgoing(program));
    }

    function_states[function_id] = FunctionState::Complete;
    postorder.push(function_id);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FunctionState {
    NotStarted,
    InProgress,
    Complete,
}
