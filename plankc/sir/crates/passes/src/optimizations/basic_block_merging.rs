use hashbrown::HashMap;
use plank_core::{DenseIndexSet, Span};
use sir_data::{BasicBlockId, Control, EthIRProgram, FunctionId, LocalId, Operation};

use crate::{AnalysesMask, AnalysesStore, Pass, Predecessors, analyses::ReachableBlocks};

pub struct BasicBlockMerger {
    entries: DenseIndexSet<BasicBlockId>,
    visited_blocks: DenseIndexSet<BasicBlockId>,
    visited_functions: DenseIndexSet<FunctionId>,
    worklist: Vec<BasicBlockId>,
    input_remap: HashMap<LocalId, LocalId>,
}

impl Pass for BasicBlockMerger {
    fn run(&mut self, program: &mut EthIRProgram, store: &AnalysesStore) {
        self.visited_blocks.clear();
        self.visited_functions.clear();
        assert!(self.worklist.is_empty());
        self.entries.clear();
        for function in program.functions.iter() {
            self.entries.add(function.entry());
        }
        let mut predecessors = store.predecessors_mut(program);
        let mut reachable_blocks = store.reachable_blocks_mut(program, true);

        self.visit_function(program.init_entry, program, &mut predecessors, &mut reachable_blocks);
        if let Some(main_entry) = program.main_entry {
            self.visit_function(main_entry, program, &mut predecessors, &mut reachable_blocks);
        }

        drop(predecessors);
        drop(reachable_blocks);
        store.predecessors.mark_valid();
        store.reachable_blocks.mark_valid();
    }

    fn preserves(&self) -> AnalysesMask {
        AnalysesMask::FunctionEffects
            | AnalysesMask::Predecessors
            | AnalysesMask::ReachableBlocks
            | AnalysesMask::ReachableFunctions
    }
}

impl BasicBlockMerger {
    fn visit_function(
        &mut self,
        fn_id: FunctionId,
        program: &mut EthIRProgram,
        predecessors: &mut Predecessors,
        reachable_blocks: &mut ReachableBlocks,
    ) {
        if !self.visited_functions.add(fn_id) {
            return;
        }

        self.worklist.push(program.functions[fn_id].entry());
        while let Some(curr) = self.worklist.pop() {
            if !self.visited_blocks.add(curr) {
                continue;
            }

            for op_id in program.basic_blocks[curr].operations {
                if let Operation::InternalCall(data) = program.operations[op_id] {
                    self.visit_function(data.function, program, predecessors, reachable_blocks);
                }
            }

            match program.basic_blocks[curr].control {
                Control::ContinuesTo(succ)
                    if curr != succ
                        && predecessors.of(succ) == [curr]
                        && !self.entries.contains(succ) =>
                {
                    self.merge_blocks(curr, succ, program, predecessors, reachable_blocks);
                    assert!(self.visited_blocks.remove(curr));
                    self.worklist.push(curr);
                }
                _ => {
                    self.worklist.extend(program.block(curr).successors());
                }
            }
        }
    }

    fn merge_blocks(
        &mut self,
        pred: BasicBlockId,
        succ: BasicBlockId,
        program: &mut EthIRProgram,
        predecessors: &mut Predecessors,
        reachable_blocks: &mut ReachableBlocks,
    ) {
        self.input_remap.clear();
        for (&succ_input, &pred_output) in
            program.block(succ).inputs().iter().zip(program.block(pred).outputs())
        {
            assert!(self.input_remap.insert(succ_input, pred_output).is_none());
        }

        let operations_start = program.operations.next_idx();
        for op in program.basic_blocks[pred].operations {
            program.clone_operation(op);
        }
        for op in program.basic_blocks[succ].operations {
            let new_idx = program.clone_operation(op);
            let operation = &mut program.operations[new_idx];
            for input in operation.inputs_mut(&mut program.locals) {
                if let Some(&replacement) = self.input_remap.get(input) {
                    *input = replacement;
                }
            }
        }
        program.basic_blocks[pred].operations =
            Span::new(operations_start, program.operations.next_idx());

        let outputs_start = program.locals.next_idx();
        for idx in program.basic_blocks[succ].outputs {
            let output = program.locals[idx];
            let remapped = self.input_remap.get(&output).copied().unwrap_or(output);
            program.locals.push(remapped);
        }
        program.basic_blocks[pred].outputs = Span::new(outputs_start, program.locals.next_idx());

        let control = match program.basic_blocks[succ].control {
            Control::Branches(mut branch) => {
                branch.condition =
                    self.input_remap.get(&branch.condition).copied().unwrap_or(branch.condition);
                Control::Branches(branch)
            }
            Control::Switch(mut switch) => {
                switch.condition =
                    self.input_remap.get(&switch.condition).copied().unwrap_or(switch.condition);
                Control::Switch(switch)
            }
            control => control,
        };
        program.basic_blocks[pred].control = control;

        for s in program.block(succ).successors() {
            predecessors.replace_predecessor_edge(s, succ, pred);
        }
        predecessors.clear_predecessors(succ);
        assert!(reachable_blocks.set_mut().remove(succ), "merged block should be reachable");
    }
}
