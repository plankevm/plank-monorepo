use plank_core::DenseIndexSet;
use sir_data::{BasicBlockId, EthIRProgram};

pub fn dfs_postorder(
    program: &EthIRProgram,
    entry: BasicBlockId,
    visited: &mut DenseIndexSet<BasicBlockId>,
    postorder: &mut Vec<BasicBlockId>,
) {
    if visited.contains(entry) {
        return;
    }
    visited.add(entry);
    for succ in program.block(entry).successors() {
        dfs_postorder(program, succ, visited, postorder);
    }
    postorder.push(entry);
}
