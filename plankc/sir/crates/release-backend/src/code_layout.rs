use plank_core::{DenseIndexMap, DenseIndexSet};
use sir_data::{BasicBlockId, ControlView, EthIRProgram, FunctionId, Operation};
use sir_stack_scheduling::{ScheduledOps, stack::StackOps};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fallthrough {
    Block(BasicBlockId),
    SwitchCase { block: BasicBlockId, case_index: usize },
}

impl Fallthrough {
    pub fn target(self) -> BasicBlockId {
        match self {
            Self::Block(block) | Self::SwitchCase { block, .. } => block,
        }
    }
}

pub(crate) struct CodeLayout {
    blocks: Vec<BasicBlockId>,
    fallthroughs: DenseIndexMap<BasicBlockId, Fallthrough>,
    fallthrough_targets: DenseIndexSet<BasicBlockId>,
    visited: DenseIndexSet<BasicBlockId>,
    worklist: Vec<BasicBlockId>,
}

impl CodeLayout {
    pub fn new(
        block_capacity: usize,
        mut visited: DenseIndexSet<BasicBlockId>,
        mut worklist: Vec<BasicBlockId>,
    ) -> Self {
        visited.clear();
        worklist.clear();
        Self {
            blocks: Vec::with_capacity(block_capacity),
            fallthroughs: DenseIndexMap::with_capacity(block_capacity),
            fallthrough_targets: DenseIndexSet::with_capacity_in_bits(block_capacity),
            visited,
            worklist,
        }
    }

    pub fn compute(&mut self, ir: &EthIRProgram, ops: &ScheduledOps, entrypoint: FunctionId) {
        self.blocks.clear();
        self.fallthroughs.clear();
        self.fallthrough_targets.clear();
        self.visited.clear();
        self.worklist.clear();

        let entry = ir.function(entrypoint).entry().id();
        assert!(self.enqueue(entry));

        while let Some(bb_id) = self.worklist.pop() {
            self.blocks.push(bb_id);

            for &stack_op in ops.get(bb_id).expect("reachable block not scheduled") {
                let op_idx = match stack_op {
                    StackOps::Op(op_idx) | StackOps::Flipped(op_idx) => op_idx,
                    _ => continue,
                };
                match ir.operations[op_idx] {
                    Operation::InternalCall(call) => {
                        self.enqueue(ir.function(call.function).entry().id());
                    }
                    Operation::InternalCallNever(call) => {
                        self.enqueue(ir.function(call.function).entry().id());
                    }
                    _ => {}
                }
            }

            let block = ir.block(bb_id);
            match block.control() {
                ControlView::LastOpTerminates | ControlView::InternalReturn => {}
                ControlView::ContinuesTo(to) => {
                    if self.enqueue(to) {
                        self.select_fallthrough(bb_id, Fallthrough::Block(to));
                    }
                }
                ControlView::Branches { condition: _, non_zero_target, zero_target } => {
                    self.enqueue(non_zero_target);
                    if self.enqueue(zero_target) {
                        self.select_fallthrough(bb_id, Fallthrough::Block(zero_target));
                    }
                }
                ControlView::Switch(switch) => {
                    let (fallthrough_target, fallthrough_case) = if let Some(target) =
                        switch.fallback()
                        && !self.visited.contains(target)
                    {
                        (Some(target), None)
                    } else if let Some((idx, (_, target))) = switch
                        .cases()
                        .enumerate()
                        .find(|(_, (_, target))| !self.visited.contains(*target))
                    {
                        (Some(target), Some(idx))
                    } else {
                        (None, None)
                    };

                    for (case_idx, (_, to)) in switch.cases().enumerate() {
                        if fallthrough_case == Some(case_idx) {
                            continue;
                        }
                        assert_ne!(fallthrough_target, Some(to));
                        self.enqueue(to);
                    }

                    if let Some(to) = fallthrough_target {
                        assert!(self.enqueue(to));
                        let fallthrough = match fallthrough_case {
                            Some(case_index) => Fallthrough::SwitchCase { block: to, case_index },
                            None => Fallthrough::Block(to),
                        };
                        self.select_fallthrough(bb_id, fallthrough);
                    }
                }
            }

            for successor in block.successors() {
                self.enqueue(successor);
            }
        }

        for (index, &source) in self.blocks.iter().enumerate() {
            if let Some(fallthrough) = self.selected_fallthrough(source) {
                assert_eq!(
                    self.blocks.get(index + 1),
                    Some(&fallthrough.target()),
                    "invariant: selected fallthrough target is not emitted after its source"
                );
            }
        }
    }

    pub fn blocks(&self) -> &[BasicBlockId] {
        &self.blocks
    }

    pub fn selected_fallthrough(&self, source: BasicBlockId) -> Option<Fallthrough> {
        self.fallthroughs.get(source).copied()
    }

    pub fn is_fallthrough_target(&self, block: BasicBlockId) -> bool {
        self.fallthrough_targets.contains(block)
    }

    fn enqueue(&mut self, block: BasicBlockId) -> bool {
        if self.visited.add(block) {
            self.worklist.push(block);
            true
        } else {
            false
        }
    }

    fn select_fallthrough(&mut self, source: BasicBlockId, fallthrough: Fallthrough) {
        self.fallthroughs.insert_no_prev(source, fallthrough);
        assert!(self.fallthrough_targets.add(fallthrough.target()));
    }
}
