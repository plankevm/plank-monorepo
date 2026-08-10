use hashbrown::HashMap;
use plank_core::{DenseIndexSet, Span};
use sir_data::{BasicBlockId, Control, EthIRProgram, LocalId};

use crate::{AnalysesMask, AnalysesStore, Pass, Predecessors};

pub struct BasicBlockMerger {
    entries: DenseIndexSet<BasicBlockId>,
    input_remap: HashMap<LocalId, LocalId>,
}

impl Pass for BasicBlockMerger {
    fn run(&mut self, program: &mut EthIRProgram, store: &AnalysesStore) {
        self.entries.clear();
        let rpo = store.reverse_post_order(program);
        for &fn_id in rpo.functions_rpo() {
            self.entries.add(program.functions[fn_id].entry());
        }
        let mut predecessors = store.predecessors_mut(program);

        for &curr in rpo.blocks_postorder() {
            match program.basic_blocks[curr].control {
                Control::ContinuesTo(succ)
                    if curr != succ
                        && predecessors.of(succ) == [curr]
                        && !self.entries.contains(succ) =>
                {
                    self.merge_blocks(curr, succ, program, &mut predecessors);
                }
                _ => {}
            }
        }

        drop(predecessors);
        store.predecessors.mark_valid();
    }

    fn preserves(&self) -> AnalysesMask {
        AnalysesMask::FunctionEffects | AnalysesMask::Predecessors
    }
}

impl BasicBlockMerger {
    fn merge_blocks(
        &mut self,
        pred: BasicBlockId,
        succ: BasicBlockId,
        program: &mut EthIRProgram,
        predecessors: &mut Predecessors,
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
    }
}
