use plank_core::{DenseIndexMap, DenseIndexSet};
use sir_data::{BasicBlockId, CasesId, ControlView, EthIRProgram, Operation};

#[derive(Clone, Copy)]
struct ControlCandidate {
    source: BasicBlockId,
    target: BasicBlockId,
}

#[derive(Clone, Copy)]
struct SwitchCandidate {
    source: BasicBlockId,
    fallback: Option<BasicBlockId>,
    cases: CasesId,
}

pub(crate) struct CodeLayout {
    entry_block: BasicBlockId,
    entrypoint_blocks: DenseIndexSet<BasicBlockId>,
    control_candidates: Vec<ControlCandidate>,
    switch_candidates: Vec<SwitchCandidate>,
    assigned_successors: DenseIndexMap<BasicBlockId, BasicBlockId>,
    with_assigned_predecessor: DenseIndexSet<BasicBlockId>,
    worklist: Vec<BasicBlockId>,
}

impl CodeLayout {
    pub fn new(
        entry_block: BasicBlockId,
        mut entrypoint_blocks: DenseIndexSet<BasicBlockId>,
        mut worklist: Vec<BasicBlockId>,
    ) -> Self {
        entrypoint_blocks.clear();
        worklist.clear();
        Self {
            entry_block,
            entrypoint_blocks,
            control_candidates: Vec::new(),
            switch_candidates: Vec::new(),
            assigned_successors: DenseIndexMap::new(),
            with_assigned_predecessor: DenseIndexSet::new(),
            worklist,
        }
    }

    pub fn compute(&mut self, ir: &EthIRProgram, entry_block: BasicBlockId) {
        self.entry_block = entry_block;
        self.discover_candidates(ir);
        self.build_layout(ir);
    }

    pub fn entry_block(&self) -> BasicBlockId {
        self.entry_block
    }

    pub fn blocks_without_assigned_predecessor(&self) -> impl Iterator<Item = BasicBlockId> + '_ {
        self.entrypoint_blocks.iter().filter(|&block| !self.has_assigned_predecessor(block))
    }

    pub fn assigned_successor(&self, block: BasicBlockId) -> Option<BasicBlockId> {
        self.assigned_successors.get(block).copied()
    }

    pub fn has_assigned_predecessor(&self, block: BasicBlockId) -> bool {
        self.with_assigned_predecessor.contains(block)
    }

    fn discover_candidates(&mut self, ir: &EthIRProgram) {
        self.entrypoint_blocks.clear();
        self.control_candidates.clear();
        self.switch_candidates.clear();
        self.worklist.clear();

        self.enqueue_block(self.entry_block);

        while let Some(source) = self.worklist.pop() {
            let block = ir.block(source);
            for operation in block.operations() {
                let callee = match operation.op() {
                    Operation::InternalCall(call) => call.function,
                    Operation::InternalCallNever(call) => call.function,
                    _ => continue,
                };

                self.enqueue_block(ir.function(callee).entry().id());
            }

            match block.control() {
                ControlView::ContinuesTo(target) => {
                    self.control_candidates.push(ControlCandidate { source, target });
                    self.enqueue_block(target);
                }
                ControlView::Branches { zero_target, non_zero_target, .. } => {
                    self.control_candidates.push(ControlCandidate { source, target: zero_target });
                    self.enqueue_block(non_zero_target);
                    self.enqueue_block(zero_target);
                }
                ControlView::Switch(switch) => {
                    let cases = switch.cases_id();
                    for &target in ir.cases[cases].get_bb_ids(ir) {
                        self.enqueue_block(target);
                    }
                    let fallback = switch.fallback();
                    if let Some(target) = fallback {
                        self.enqueue_block(target);
                    }
                    self.switch_candidates.push(SwitchCandidate { source, fallback, cases });
                }
                ControlView::LastOpTerminates | ControlView::InternalReturn => {}
            }
        }
    }

    fn enqueue_block(&mut self, block: BasicBlockId) {
        if self.entrypoint_blocks.add(block) {
            self.worklist.push(block);
        }
    }

    fn build_layout(&mut self, ir: &EthIRProgram) {
        self.assigned_successors.clear();
        self.with_assigned_predecessor.clear();

        let mut try_assign_successor = |source, target| {
            if target == self.entry_block || self.with_assigned_predecessor.contains(target) {
                return false;
            }

            // Reject cycles: target's successor chain must not reach source
            let mut block = target;
            loop {
                if block == source {
                    return false;
                }
                let Some(&next) = self.assigned_successors.get(block) else {
                    break;
                };
                block = next;
            }

            self.assigned_successors.insert_no_prev(source, target);
            assert!(self.with_assigned_predecessor.add(target));
            true
        };

        for &ControlCandidate { source, target } in &self.control_candidates {
            try_assign_successor(source, target);
        }

        for &SwitchCandidate { source, fallback, cases } in &self.switch_candidates {
            if let Some(target) = fallback
                && try_assign_successor(source, target)
            {
                continue;
            }

            for &target in ir.cases[cases].get_bb_ids(ir) {
                if try_assign_successor(source, target) {
                    break;
                }
            }
        }
    }
}
