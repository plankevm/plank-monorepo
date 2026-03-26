use hashbrown::{HashMap, HashSet};

use crate::analyses::{AnalysesStore, Predecessors, cache::Analysis, dfs_postorder};
use plank_core::{DenseIndexSet, Idx};
use sir_data::{BasicBlockId, ControlView, EthIRProgram, IndexVec, LocalId, OperationIdx};
use std::cmp::{Ord, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalStart {
    BlockStart,
    At(OperationIdx),
}

impl PartialOrd for IntervalStart {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IntervalStart {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (a, b) if a == b => Ordering::Equal,
            (Self::BlockStart, _) => Ordering::Less,
            (_, Self::BlockStart) => Ordering::Greater,
            (Self::At(op_idx1), Self::At(op_idx2)) => op_idx1.cmp(op_idx2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalEnd {
    At(OperationIdx),
    BlockEnd,
}

impl PartialOrd for IntervalEnd {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IntervalEnd {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (a, b) if a == b => Ordering::Equal,
            (Self::BlockEnd, _) => Ordering::Greater,
            (_, Self::BlockEnd) => Ordering::Less,
            (Self::At(op_idx1), Self::At(op_idx2)) => op_idx1.cmp(op_idx2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: IntervalStart,
    pub end: IntervalEnd,
}

impl PartialOrd for Interval {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Interval {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.start.cmp(&other.start).then_with(|| self.end.cmp(&other.end))
    }
}

pub type LocalIntervals = Vec<(BasicBlockId, Interval)>;

#[derive(Debug, Clone, Default)]
pub struct LocalLiveness {
    local_intervals: IndexVec<LocalId, LocalIntervals>,
    locals_live_at_entry: IndexVec<BasicBlockId, HashSet<LocalId>>,
    locals_live_at_exit: IndexVec<BasicBlockId, HashSet<LocalId>>,
}

impl Analysis for LocalLiveness {
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        self.local_intervals.clear();
        self.local_intervals.resize_with(program.next_free_local_id.idx(), LocalIntervals::default);
        self.locals_live_at_entry.clear();
        self.locals_live_at_entry.resize(program.basic_blocks.len(), HashSet::new());
        self.locals_live_at_exit.clear();
        self.locals_live_at_exit.resize(program.basic_blocks.len(), HashSet::new());

        let predecessors = store.predecessors(program);

        let mut blocks_postorder = Vec::new();
        let mut visited = DenseIndexSet::new();
        for func in program.functions_iter() {
            dfs_postorder(program, func.entry().id(), &mut visited, &mut blocks_postorder);
        }

        self.compute_liveness(program, &predecessors, &blocks_postorder);
        self.compute_intervals(program);
    }
}

impl LocalLiveness {
    pub fn intervals_of(&self, local: LocalId) -> &[(BasicBlockId, Interval)] {
        &self.local_intervals[local]
    }

    fn compute_liveness(
        &mut self,
        program: &EthIRProgram,
        predecessors: &Predecessors,
        blocks_postorder: &[BasicBlockId],
    ) {
        let mut changed = true;
        while changed {
            changed = false;
            for &bb_id in blocks_postorder {
                Self::compute_liveness_at_block_entry(
                    program,
                    bb_id,
                    &self.locals_live_at_exit[bb_id],
                    &mut self.locals_live_at_entry[bb_id],
                );

                changed |= Self::propagate_liveness_to_predecessors(
                    program,
                    predecessors.of(bb_id),
                    program.block(bb_id).inputs(),
                    &self.locals_live_at_entry[bb_id],
                    &mut self.locals_live_at_exit,
                );
            }
        }
    }

    fn compute_liveness_at_block_entry(
        program: &EthIRProgram,
        bb_id: BasicBlockId,
        exit_liveness: &HashSet<LocalId>,
        entry_liveness: &mut HashSet<LocalId>,
    ) {
        let block = &program.basic_blocks[bb_id];
        entry_liveness.clone_from(exit_liveness);

        match program.block(bb_id).control() {
            ControlView::Branches { condition, .. } => {
                entry_liveness.insert(condition);
            }
            ControlView::Switch(switch) => {
                entry_liveness.insert(switch.condition());
            }
            _ => {}
        }

        for op_idx in block.operations.iter().rev() {
            let op = program.operations[op_idx];
            for out in op.outputs(program) {
                entry_liveness.remove(out);
            }
            for input in op.inputs(program) {
                entry_liveness.insert(*input);
            }
        }
    }

    fn propagate_liveness_to_predecessors(
        program: &EthIRProgram,
        predecessor_ids: &[BasicBlockId],
        block_inputs: &[LocalId],
        entry_liveness: &HashSet<LocalId>,
        locals_live_at_exit: &mut IndexVec<BasicBlockId, HashSet<LocalId>>,
    ) -> bool {
        let mut changed = false;
        for &pred_id in predecessor_ids {
            let pred_outputs = program.block(pred_id).outputs();
            for &local in entry_liveness {
                // Block inputs are propagated as the corresponding predecessor output
                if let Some(pos) = block_inputs.iter().position(|&i| i == local) {
                    changed |= locals_live_at_exit[pred_id].insert(pred_outputs[pos]);
                } else {
                    changed |= locals_live_at_exit[pred_id].insert(local);
                }
            }
        }
        changed
    }

    fn compute_intervals(&mut self, program: &EthIRProgram) {
        let mut local_interval_ends: HashMap<LocalId, IntervalEnd> = HashMap::new();

        for block in program.blocks() {
            debug_assert!(local_interval_ends.is_empty());
            let bb_id = block.id();

            for local in &self.locals_live_at_exit[bb_id] {
                local_interval_ends.insert(*local, IntervalEnd::BlockEnd);
            }

            match block.control() {
                ControlView::Branches { condition, .. } => {
                    local_interval_ends.insert(condition, IntervalEnd::BlockEnd);
                }
                ControlView::Switch(switch) => {
                    local_interval_ends.insert(switch.condition(), IntervalEnd::BlockEnd);
                }
                _ => {}
            }

            for op in block.operations().rev() {
                for out in op.op().outputs(program) {
                    if let Some(end) = local_interval_ends.remove(out) {
                        self.local_intervals[*out]
                            .push((bb_id, Interval { start: IntervalStart::At(op.id()), end }));
                    }
                }

                for input in op.op().inputs(program) {
                    local_interval_ends.entry(*input).or_insert(IntervalEnd::At(op.id()));
                }
            }

            for (local, end) in local_interval_ends.drain() {
                self.local_intervals[local]
                    .push((bb_id, Interval { start: IntervalStart::BlockStart, end }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyses::AnalysesStore;
    use sir_parser::{EmitConfig, parse_or_panic};

    fn assert_has_interval(
        liveness: &LocalLiveness,
        local: LocalId,
        bb: BasicBlockId,
        start: IntervalStart,
        end: IntervalEnd,
    ) {
        let found = liveness
            .intervals_of(local)
            .iter()
            .any(|&(b, iv)| b == bb && iv.start == start && iv.end == end);
        assert!(
            found,
            "expected interval ({start:?}, {end:?}) for {local:?} in {bb}, got {:?}",
            liveness.intervals_of(local)
        );
    }

    fn first_output(ir: &EthIRProgram, bb: BasicBlockId, op_n: usize) -> LocalId {
        ir.operations[op_at(ir, bb, op_n)].outputs(ir)[0]
    }

    fn op_at(ir: &EthIRProgram, bb: BasicBlockId, n: usize) -> OperationIdx {
        ir.basic_blocks[bb].operations.start + n as u32
    }

    #[test]
    fn single_block_intervals() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    v = const 42
                    mstore256 buf v
                    x = mload256 buf
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.local_liveness(&ir);

        let bb = BasicBlockId::new(0);
        let buf = first_output(&ir, bb, 0);

        assert_eq!(liveness.intervals_of(buf).len(), 1);
        assert_has_interval(
            &liveness,
            buf,
            bb,
            IntervalStart::At(op_at(&ir, bb, 0)),
            IntervalEnd::At(op_at(&ir, bb, 3)),
        );
    }

    #[test]
    fn cross_block_interval() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    => @use_it
                }
                use_it {
                    v = mload256 buf
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.local_liveness(&ir);

        let bb0 = BasicBlockId::new(0);
        let bb1 = BasicBlockId::new(1);
        let buf = first_output(&ir, bb0, 0);

        assert_has_interval(
            &liveness,
            buf,
            bb0,
            IntervalStart::At(op_at(&ir, bb0, 0)),
            IntervalEnd::BlockEnd,
        );
        assert_has_interval(
            &liveness,
            buf,
            bb1,
            IntervalStart::BlockStart,
            IntervalEnd::At(op_at(&ir, bb1, 0)),
        );
    }

    #[test]
    fn loop_liveness() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    => @loop_body
                }
                loop_body {
                    v = mload256 buf
                    cond = iszero v
                    => cond ? @done : @loop_body
                }
                done {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.local_liveness(&ir);

        let bb0 = BasicBlockId::new(0);
        let bb1 = BasicBlockId::new(1);
        let buf = first_output(&ir, bb0, 0);

        assert!(liveness.locals_live_at_exit[bb0].contains(&buf));
        assert!(liveness.locals_live_at_entry[bb1].contains(&buf));
        assert!(liveness.locals_live_at_exit[bb1].contains(&buf));

        assert_has_interval(&liveness, buf, bb1, IntervalStart::BlockStart, IntervalEnd::BlockEnd);
    }

    #[test]
    fn dead_local_no_interval() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    x = const 42
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.local_liveness(&ir);

        let x = first_output(&ir, BasicBlockId::new(0), 0);
        assert!(liveness.intervals_of(x).is_empty());
    }

    #[test]
    fn asymmetric_branch_liveness() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    x = const 1
                    cond = calldatasize
                    => cond ? @use_it : @skip
                }
                use_it {
                    y = add x x
                    => @done
                }
                skip {
                    => @done
                }
                done {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.local_liveness(&ir);

        let bb0 = BasicBlockId::new(0);
        let bb1 = BasicBlockId::new(1);
        let bb2 = BasicBlockId::new(2);
        let x = first_output(&ir, bb0, 0);

        assert!(liveness.locals_live_at_exit[bb0].contains(&x));
        assert!(liveness.locals_live_at_entry[bb1].contains(&x));
        assert!(!liveness.locals_live_at_entry[bb2].contains(&x));

        assert_has_interval(
            &liveness,
            x,
            bb0,
            IntervalStart::At(op_at(&ir, bb0, 0)),
            IntervalEnd::BlockEnd,
        );
        assert_has_interval(
            &liveness,
            x,
            bb1,
            IntervalStart::BlockStart,
            IntervalEnd::At(op_at(&ir, bb1, 0)),
        );
    }

    #[test]
    fn unused_local_with_successor() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    => @next
                }
                next {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.local_liveness(&ir);

        let buf = first_output(&ir, BasicBlockId::new(0), 0);
        assert!(liveness.intervals_of(buf).is_empty());
    }

    #[test]
    fn block_inputs_liveness() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry -> buf {
                    buf = salloc 32
                    cond = calldatasize
                    => cond ? @use_it : @skip
                }
                use_it ptr -> ptr {
                    v = mload256 ptr
                    => @skip
                }
                skip _p {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.local_liveness(&ir);

        let bb0 = BasicBlockId::new(0);
        let bb1 = BasicBlockId::new(1);
        let buf = first_output(&ir, bb0, 0);
        let ptr = ir.block(bb1).inputs()[0];

        assert!(!liveness.intervals_of(buf).is_empty());
        assert!(!liveness.intervals_of(ptr).is_empty());

        assert_has_interval(
            &liveness,
            buf,
            bb0,
            IntervalStart::At(op_at(&ir, bb0, 0)),
            IntervalEnd::BlockEnd,
        );
        assert_has_interval(
            &liveness,
            ptr,
            bb1,
            IntervalStart::BlockStart,
            IntervalEnd::At(op_at(&ir, bb1, 0)), // mload256
        );
    }

    #[test]
    fn multiple_uses_same_block() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 64
                    v = mload256 buf
                    mstore256 buf v
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.local_liveness(&ir);

        let bb0 = BasicBlockId::new(0);
        let buf = first_output(&ir, bb0, 0);

        assert_eq!(liveness.intervals_of(buf).len(), 1);
        assert_has_interval(
            &liveness,
            buf,
            bb0,
            IntervalStart::At(op_at(&ir, bb0, 0)),
            IntervalEnd::At(op_at(&ir, bb0, 2)),
        );
    }
}
