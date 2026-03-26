use crate::analyses::{
    AnalysesStore, DefUse, Interval, IntervalEnd, IntervalStart, LocalLiveness, UseKind,
    cache::Analysis,
};
use plank_core::DenseIndexMap;
use sir_data::{
    BasicBlockId, Control, EthIRProgram, IndexVec, LocalId, Operation, OperationIdx, StaticAllocId,
    newtype_index,
};

newtype_index! {
    pub struct AllocId;
}

#[derive(Debug, Clone, Copy)]
pub enum AllocKind {
    Static { size: u32, id: StaticAllocId },
    Dynamic { size_local: LocalId },
}

#[derive(Debug, Clone)]
pub struct AllocData {
    pub def_block: BasicBlockId,
    pub def_op: OperationIdx,
    pub base_ptr: LocalId,
    pub kind: AllocKind,
    pub escapes: bool,
    pub intervals: Vec<(BasicBlockId, Interval)>,
}

#[derive(Debug, Clone, Default)]
pub struct AllocationLiveness {
    pub allocations: IndexVec<AllocId, AllocData>,
    pub local_to_alloc: DenseIndexMap<LocalId, AllocId>,
}

impl Analysis for AllocationLiveness {
    /// Only tracks non-escaping allocations for now.
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        self.allocations.clear();
        self.local_to_alloc.clear();

        let def_use = store.def_use(program);
        self.discover_allocations(program, &def_use);
        if self.allocations.is_empty() {
            return;
        }

        let local_liveness = store.local_liveness(program);
        self.populate_intervals_from_local_liveness(&local_liveness);
    }
}

fn operation_causes_ptr_escape(program: &EthIRProgram, op: Operation, local: LocalId) -> bool {
    assert!(op.inputs(program).contains(&local), "expected op that uses local");
    match op {
        Operation::Keccak256(data) => {
            let [_offset, size] = data.ins;
            size == local
        }
        Operation::Balance(_) | Operation::CallDataLoad(_) => true,
        Operation::CallDataCopy(data) | Operation::CodeCopy(data) => {
            let [_dst, src, size] = data.ins;
            [src, size].contains(&local)
        }
        Operation::ExtCodeSize(_) => true,
        Operation::ExtCodeCopy(data) => {
            let &[addr, _dst, src, size] = data.get_inputs(program);
            [addr, src, size].contains(&local)
        }
        Operation::ReturnDataCopy(data) => {
            let [_dst, src, size] = data.ins;
            [src, size].contains(&local)
        }
        Operation::ExtCodeHash(_)
        | Operation::BlockHash(_)
        | Operation::BlobHash(_)
        | Operation::SLoad(_)
        | Operation::SStore(_)
        | Operation::TLoad(_)
        | Operation::TStore(_) => true,
        Operation::Create(data) => {
            let &[value, _offset, size] = data.get_inputs(program);
            [value, size].contains(&local)
        }
        Operation::Create2(data) => {
            let &[value, _offset, size, salt] = data.get_inputs(program);
            [value, size, salt].contains(&local)
        }
        Operation::Call(data) | Operation::CallCode(data) => {
            let &[gas, addr, value, _arg_offset, arg_size, _ret_offset, ret_size] =
                data.get_inputs(program);
            [gas, addr, value, arg_size, ret_size].contains(&local)
        }
        Operation::DelegateCall(data) | Operation::StaticCall(data) => {
            let &[gas, addr, _arg_offset, arg_size, _ret_offset, ret_size] =
                data.get_inputs(program);
            [gas, addr, arg_size, ret_size].contains(&local)
        }
        Operation::DynamicAllocZeroed(_) | Operation::DynamicAllocAnyBytes(_) => true,
        Operation::MemoryCopy(data) => {
            let [_dst, _src, size] = data.ins;
            size == local
        }
        Operation::MemoryStore(data) => {
            let [_ptr, value] = data.ins;
            value == local
        }
        Operation::InternalCall(_) => true,

        Operation::Add(_)
        | Operation::Mul(_)
        | Operation::Sub(_)
        | Operation::Div(_)
        | Operation::SDiv(_)
        | Operation::Mod(_)
        | Operation::SMod(_)
        | Operation::AddMod(_)
        | Operation::MulMod(_)
        | Operation::Exp(_)
        | Operation::SignExtend(_)
        | Operation::Lt(_)
        | Operation::Gt(_)
        | Operation::SLt(_)
        | Operation::SGt(_)
        | Operation::Eq(_)
        | Operation::IsZero(_)
        | Operation::And(_)
        | Operation::Or(_)
        | Operation::Xor(_)
        | Operation::Not(_)
        | Operation::Byte(_)
        | Operation::Shl(_)
        | Operation::Shr(_)
        | Operation::Sar(_)
        | Operation::Address(_)
        | Operation::Origin(_)
        | Operation::Caller(_)
        | Operation::CallValue(_)
        | Operation::CallDataSize(_)
        | Operation::CodeSize(_)
        | Operation::GasPrice(_)
        | Operation::ReturnDataSize(_)
        | Operation::Gas(_)
        | Operation::Coinbase(_)
        | Operation::Timestamp(_)
        | Operation::Number(_)
        | Operation::Difficulty(_)
        | Operation::GasLimit(_)
        | Operation::ChainId(_)
        | Operation::SelfBalance(_)
        | Operation::BaseFee(_)
        | Operation::BlobBaseFee(_)
        | Operation::Log0(_)
        | Operation::Log1(_)
        | Operation::Log2(_)
        | Operation::Log3(_)
        | Operation::Log4(_)
        | Operation::Return(_)
        | Operation::Stop(_)
        | Operation::Revert(_)
        | Operation::Invalid(_)
        | Operation::SelfDestruct(_)
        | Operation::AcquireFreePointer(_)
        | Operation::StaticAllocZeroed(_)
        | Operation::StaticAllocAnyBytes(_)
        | Operation::MemoryLoad(_)
        | Operation::SetCopy(_)
        | Operation::SetSmallConst(_)
        | Operation::SetLargeConst(_)
        | Operation::SetDataOffset(_)
        | Operation::Noop(())
        | Operation::RuntimeStartOffset(_)
        | Operation::InitEndOffset(_)
        | Operation::RuntimeLength(_) => false,
    }
}

impl AllocationLiveness {
    fn discover_allocations(&mut self, program: &EthIRProgram, def_use: &DefUse) {
        for block in program.blocks() {
            let bb_id = block.id();
            for op in block.operations() {
                let (alloc_id, base_ptr) = match op.op() {
                    Operation::StaticAllocZeroed(data) | Operation::StaticAllocAnyBytes(data) => {
                        let alloc_id = self.allocations.push(AllocData {
                            def_block: bb_id,
                            def_op: op.id(),
                            base_ptr: data.ptr_out,
                            kind: AllocKind::Static { size: data.size, id: data.alloc_id },
                            escapes: false,
                            intervals: Vec::new(),
                        });
                        (alloc_id, data.ptr_out)
                    }
                    Operation::DynamicAllocZeroed(data) | Operation::DynamicAllocAnyBytes(data) => {
                        let [size_local] = data.ins;
                        let [ptr_out] = data.outs;
                        let alloc_id = self.allocations.push(AllocData {
                            def_block: bb_id,
                            def_op: op.id(),
                            base_ptr: ptr_out,
                            kind: AllocKind::Dynamic { size_local },
                            escapes: false,
                            intervals: Vec::new(),
                        });
                        (alloc_id, ptr_out)
                    }
                    _ => continue,
                };
                assert!(self.local_to_alloc.insert(base_ptr, alloc_id).is_none());
            }
        }

        let mut worklist = Vec::new();
        for alloc_id in self.allocations.iter_idx() {
            let base_ptr = self.allocations[alloc_id].base_ptr;
            self.propagate_pointers_and_mark_escapes(
                program,
                def_use,
                alloc_id,
                base_ptr,
                &mut worklist,
            );
        }
    }

    fn propagate_pointers_and_mark_escapes(
        &mut self,
        program: &EthIRProgram,
        def_use: &DefUse,
        alloc_id: AllocId,
        ptr_local: LocalId,
        worklist: &mut Vec<LocalId>,
    ) {
        worklist.clear();
        worklist.push(ptr_local);

        while let Some(local) = worklist.pop() {
            for use_loc in def_use.uses_of(local) {
                let op = match use_loc.kind {
                    UseKind::Control => continue,
                    UseKind::BlockOutput(pos) => {
                        let block = &program.basic_blocks[use_loc.block_id];
                        if matches!(block.control, Control::InternalReturn) {
                            self.allocations[alloc_id].escapes = true;
                            continue;
                        }
                        for succ_id in program.block(use_loc.block_id).successors() {
                            let succ_input = program.block(succ_id).inputs()[pos as usize];
                            self.link_local_to_alloc(succ_input, alloc_id, worklist);
                        }
                        continue;
                    }
                    UseKind::Operation(op_idx) => program.operations[op_idx],
                };

                if can_derive_pointer(op) {
                    for &out in op.outputs(program) {
                        self.link_local_to_alloc(out, alloc_id, worklist);
                    }
                }

                self.allocations[alloc_id].escapes |=
                    operation_causes_ptr_escape(program, op, local);
            }
        }
    }

    fn link_local_to_alloc(
        &mut self,
        local: LocalId,
        alloc_id: AllocId,
        worklist: &mut Vec<LocalId>,
    ) {
        match self.local_to_alloc.insert(local, alloc_id) {
            None => worklist.push(local),
            Some(existing) => {
                if existing != alloc_id {
                    // TODO: Track variables that may be different allocations.
                    // For now just conservatively mark as escaped.
                    self.allocations[alloc_id].escapes = true;
                    self.allocations[existing].escapes = true;
                }
            }
        }
    }

    fn populate_intervals_from_local_liveness(&mut self, local_liveness: &LocalLiveness) {
        for (local, &alloc_id) in self.local_to_alloc.iter() {
            if self.allocations[alloc_id].escapes {
                continue;
            }
            for &(bb_id, interval) in local_liveness.intervals_of(local) {
                self.allocations[alloc_id].intervals.push((bb_id, interval));
            }
        }

        for alloc in self.allocations.iter_mut() {
            if alloc.escapes {
                continue;
            }
            merge_intervals(&mut alloc.intervals);
        }
    }
}

fn can_derive_pointer(op: Operation) -> bool {
    matches!(
        op,
        Operation::Add(_)
            | Operation::Mul(_)
            | Operation::Sub(_)
            | Operation::Div(_)
            | Operation::SDiv(_)
            | Operation::Mod(_)
            | Operation::SMod(_)
            | Operation::AddMod(_)
            | Operation::MulMod(_)
            | Operation::Exp(_)
            | Operation::SignExtend(_)
            | Operation::And(_)
            | Operation::Or(_)
            | Operation::Xor(_)
            | Operation::Not(_)
            | Operation::Byte(_)
            | Operation::Shl(_)
            | Operation::Shr(_)
            | Operation::Sar(_)
            | Operation::SetCopy(_)
    )
}

fn merge_intervals(intervals: &mut Vec<(BasicBlockId, Interval)>) {
    if intervals.len() <= 1 {
        return;
    }

    intervals.sort();

    let mut dst = 0;
    for src in 1..intervals.len() {
        let (prev_bb, prev_interval) = intervals[dst];
        let (cur_bb, cur_interval) = intervals[src];

        let overlaps = prev_bb == cur_bb
            && match (prev_interval.end, cur_interval.start) {
                (IntervalEnd::BlockEnd, _) | (_, IntervalStart::BlockStart) => true,
                (IntervalEnd::At(prev_end), IntervalStart::At(cur_start)) => cur_start <= prev_end,
            };

        if overlaps {
            intervals[dst].1.end = prev_interval.end.max(cur_interval.end);
        } else {
            // [..., A,      b, ..., F]
            //    dst^ merged^    src^
            // We want the final array to be contiguous and the next `dst` to point to `src`,
            // so we not only bump `dst` but also copy `src` back.
            dst += 1;
            intervals[dst] = intervals[src];
        }
    }
    intervals.truncate(dst + 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyses::{IntervalEnd, IntervalStart};
    use sir_parser::{EmitConfig, parse_or_panic};

    fn get_alloc(liveness: &AllocationLiveness, idx: u32) -> &AllocData {
        &liveness.allocations[AllocId::new(idx)]
    }

    fn op_idx_in_block(ir: &EthIRProgram, bb: BasicBlockId, n: usize) -> OperationIdx {
        ir.basic_blocks[bb].operations.iter().nth(n).expect("operation index out of bounds")
    }

    fn assert_has_interval(
        alloc: &AllocData,
        bb: BasicBlockId,
        start: IntervalStart,
        end: IntervalEnd,
    ) {
        let found =
            alloc.intervals.iter().any(|&(b, iv)| b == bb && iv.start == start && iv.end == end);
        assert!(found, "expected ({start:?}, {end:?}) in {bb}, got {:?}", alloc.intervals);
    }

    #[test]
    fn single_alloc_straight_line() {
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
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 1);
        let alloc = get_alloc(&liveness, 0);
        assert!(!alloc.escapes);
        assert_eq!(alloc.intervals.len(), 1);
        assert_has_interval(
            alloc,
            BasicBlockId::new(0),
            IntervalStart::At(op_idx_in_block(&ir, BasicBlockId::new(0), 0)), // salloc
            IntervalEnd::At(op_idx_in_block(&ir, BasicBlockId::new(0), 3)),   // mload256
        );
    }

    #[test]
    fn multiple_allocs_same_block() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    a = salloc 32
                    sz = const 64
                    b = malloc sz
                    v = mload256 a
                    mstore256 b v
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 2);

        let alloc0 = get_alloc(&liveness, 0);
        let alloc1 = get_alloc(&liveness, 1);
        assert_eq!(alloc0.intervals.len(), 1);
        assert_eq!(alloc1.intervals.len(), 1);

        assert_has_interval(
            alloc0,
            BasicBlockId::new(0),
            IntervalStart::At(op_idx_in_block(&ir, BasicBlockId::new(0), 0)), // salloc 32
            IntervalEnd::At(op_idx_in_block(&ir, BasicBlockId::new(0), 3)),   // mload256
        );
        assert_has_interval(
            alloc1,
            BasicBlockId::new(0),
            IntervalStart::At(op_idx_in_block(&ir, BasicBlockId::new(0), 2)), // malloc
            IntervalEnd::At(op_idx_in_block(&ir, BasicBlockId::new(0), 4)),   // mstore256
        );
    }

    #[test]
    fn branching_alloc_one_side() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry -> buf {
                    buf = salloc 32
                    cond = calldatasize
                    => cond ? @then : @done
                }
                then ptr -> ptr {
                    v = mload256 ptr
                    => @done
                }
                done _p {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 1);
        let alloc = get_alloc(&liveness, 0);
        assert!(!alloc.escapes);
        assert_eq!(alloc.intervals.len(), 2);
        assert_has_interval(
            alloc,
            BasicBlockId::new(0),
            IntervalStart::At(op_idx_in_block(&ir, BasicBlockId::new(0), 0)), // salloc
            IntervalEnd::BlockEnd,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(1),
            IntervalStart::BlockStart,
            IntervalEnd::At(op_idx_in_block(&ir, BasicBlockId::new(1), 0)), // mload256
        );
    }

    #[test]
    fn merge_block_alloc_from_both_predecessors() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    cond = calldatasize
                    => cond ? @left : @right
                }
                left {
                    => @merge
                }
                right {
                    => @merge
                }
                merge {
                    v = mload256 buf
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 1);
        let alloc = get_alloc(&liveness, 0);
        assert!(!alloc.escapes);
        assert_eq!(alloc.intervals.len(), 4);
        assert_has_interval(
            alloc,
            BasicBlockId::new(0),
            IntervalStart::At(op_idx_in_block(&ir, BasicBlockId::new(0), 0)), // salloc
            IntervalEnd::BlockEnd,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(1),
            IntervalStart::BlockStart,
            IntervalEnd::BlockEnd,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(2),
            IntervalStart::BlockStart,
            IntervalEnd::BlockEnd,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(3),
            IntervalStart::BlockStart,
            IntervalEnd::At(op_idx_in_block(&ir, BasicBlockId::new(3), 0)), // mload256
        );
    }

    #[test]
    fn loop_alloc_defined_outside() {
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
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 1);
        let alloc = get_alloc(&liveness, 0);
        assert!(!alloc.escapes);
        assert_eq!(alloc.intervals.len(), 2);
        assert_has_interval(
            alloc,
            BasicBlockId::new(0),
            IntervalStart::At(op_idx_in_block(&ir, BasicBlockId::new(0), 0)), // salloc
            IntervalEnd::BlockEnd,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(1),
            IntervalStart::BlockStart,
            IntervalEnd::BlockEnd,
        );
    }

    #[test]
    fn no_allocations() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    x = const 1
                    y = const 2
                    z = add x y
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 0);
    }

    #[test]
    fn derived_pointer_arithmetic() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 64
                    off = const 32
                    derived = add buf off
                    v = mload256 derived
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 1);
        let alloc = get_alloc(&liveness, 0);
        assert!(!alloc.escapes);
        assert_eq!(
            alloc.intervals,
            &[(
                BasicBlockId::new(0),
                Interval {
                    start: IntervalStart::At(op_idx_in_block(&ir, BasicBlockId::new(0), 0)), /* salloc */
                    end: IntervalEnd::At(op_idx_in_block(&ir, BasicBlockId::new(0), 3)), // mload256
                }
            )]
        );
    }

    #[test]
    fn dead_allocation() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 1);
        let alloc = get_alloc(&liveness, 0);
        assert!(alloc.intervals.is_empty());
    }

    #[test]
    fn aliased_pointers_both_escape() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    a = salloc 32
                    sz = const 64
                    b = malloc sz
                    merged = add a b
                    mstore256 merged merged
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 2);
        assert!(get_alloc(&liveness, 0).escapes);
        assert!(get_alloc(&liveness, 1).escapes);
        assert_eq!(get_alloc(&liveness, 0).intervals, &[]);
        assert_eq!(get_alloc(&liveness, 1).intervals, &[]);
    }

    #[test]
    fn pointer_stored_to_memory_escapes() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    scratch = salloc 32
                    mstore256 scratch buf
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 2);
        assert!(get_alloc(&liveness, 0).escapes, "pointer stored as value should escape");
        assert!(!get_alloc(&liveness, 1).escapes, "pointer used as address should not escape");
    }
}
