use hashbrown::HashMap;
use smallvec::SmallVec;

use crate::analyses::{
    AnalysesStore, DefUse, Predecessors, UseKind, cache::Analysis, dfs_postorder,
};
use plank_core::{DenseIndexMap, DenseIndexSet};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntervalStart {
    LiveIn,
    At(OperationIdx),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IntervalEnd {
    LiveOut,
    At(OperationIdx),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Interval {
    pub start: IntervalStart,
    pub end: IntervalEnd,
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
    pub block_exit_liveness: IndexVec<BasicBlockId, DenseIndexSet<AllocId>>,
}

impl Analysis for AllocationLiveness {
    /// Only tracks non-escaping allocations for now.
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        self.allocations.clear();
        self.local_to_alloc.clear();
        self.block_exit_liveness.clear();
        self.block_exit_liveness.resize(program.basic_blocks.len(), DenseIndexSet::new());

        let def_use = store.def_use(program);
        self.discover_allocations(program, &def_use);
        if self.allocations.is_empty() {
            return;
        }

        let local_to_input_origins = propagate_block_input_origins(program, &def_use);

        let mut blocks_postorder = Vec::new();
        let mut visited = DenseIndexSet::new();
        for func in program.functions_iter() {
            dfs_postorder(program, func.entry().id(), &mut visited, &mut blocks_postorder);
        }

        let predecessors = store.predecessors(program);

        self.compute_block_exit_liveness(
            program,
            &local_to_input_origins,
            &predecessors,
            &blocks_postorder,
        );

        self.populate_allocation_intervals(program, &local_to_input_origins, &predecessors);
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
                        let alloc_id = self.allocations.push(AllocData {
                            def_block: bb_id,
                            def_op: op.id(),
                            base_ptr: data.outs[0],
                            kind: AllocKind::Dynamic { size_local: data.ins[0] },
                            escapes: false,
                            intervals: Vec::new(),
                        });
                        (alloc_id, data.outs[0])
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
                    UseKind::BlockOutput => {
                        let block = &program.basic_blocks[use_loc.block_id];
                        if matches!(block.control, Control::InternalReturn) {
                            self.allocations[alloc_id].escapes = true;
                        }
                        continue;
                    }
                    UseKind::Operation(op_idx) => program.operations[op_idx],
                };

                if can_derive_pointer(op) {
                    for &out in op.outputs(program) {
                        match self.local_to_alloc.insert(out, alloc_id) {
                            None => worklist.push(out),
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
                }

                self.allocations[alloc_id].escapes |=
                    operation_causes_ptr_escape(program, op, local);
            }
        }
    }

    fn compute_block_exit_liveness(
        &mut self,
        program: &EthIRProgram,
        local_to_input_origins: &HashMap<LocalId, BlockInputOrigins>,
        predecessors: &Predecessors,
        blocks_postorder: &[BasicBlockId],
    ) {
        let mut input_live_flags = SmallVec::<[bool; 8]>::new();
        let mut entry_liveness = DenseIndexSet::new();
        let mut changed = true;
        while changed {
            changed = false;

            for &bb_id in blocks_postorder {
                self.compute_block_entry_liveness(program, bb_id, &mut entry_liveness);

                populate_input_live_flags(
                    program,
                    local_to_input_origins,
                    bb_id,
                    &mut input_live_flags,
                );

                changed |= self.propagate_alloc_liveness_to_predecessors(
                    program,
                    predecessors.of(bb_id),
                    &entry_liveness,
                    &input_live_flags,
                );
            }
        }
    }

    fn compute_block_entry_liveness(
        &self,
        program: &EthIRProgram,
        bb_id: BasicBlockId,
        entry_liveness: &mut DenseIndexSet<AllocId>,
    ) {
        let block = &program.basic_blocks[bb_id];
        entry_liveness.clone_from(&self.block_exit_liveness[bb_id]);

        for op_idx in block.operations.iter().rev() {
            let op = program.operations[op_idx];

            match op {
                Operation::StaticAllocZeroed(data) | Operation::StaticAllocAnyBytes(data) => {
                    if let Some(&alloc_id) = self.local_to_alloc.get(data.ptr_out) {
                        entry_liveness.remove(alloc_id);
                    }
                }
                Operation::DynamicAllocZeroed(data) | Operation::DynamicAllocAnyBytes(data) => {
                    let [ptr] = data.outs;
                    if let Some(&alloc_id) = self.local_to_alloc.get(ptr) {
                        entry_liveness.remove(alloc_id);
                    }
                }
                _ => {}
            }

            for input in op.inputs(program) {
                let Some(&alloc_id) = self.local_to_alloc.get(*input) else { continue };
                if !self.allocations[alloc_id].escapes {
                    entry_liveness.add(alloc_id);
                }
            }
        }
    }

    fn propagate_alloc_liveness_to_predecessors(
        &mut self,
        program: &EthIRProgram,
        predecessors: &[BasicBlockId],
        entry_liveness: &DenseIndexSet<AllocId>,
        input_live_flags: &[bool],
    ) -> bool {
        let mut changed = false;

        for &pred_id in predecessors {
            let pred_exit = &mut self.block_exit_liveness[pred_id];
            changed |= pred_exit.union_with(entry_liveness);

            let pred_outputs = program.block(pred_id).outputs();
            for (pos, &live) in input_live_flags.iter().enumerate() {
                if !live {
                    continue;
                }
                if let Some(&alloc_id) = self.local_to_alloc.get(pred_outputs[pos])
                    && !self.allocations[alloc_id].escapes
                    && pred_exit.add(alloc_id)
                {
                    changed = true;
                }
            }
        }

        changed
    }

    fn populate_allocation_intervals(
        &mut self,
        program: &EthIRProgram,
        local_to_input_origins: &HashMap<LocalId, BlockInputOrigins>,
        predecessors: &Predecessors,
    ) {
        let mut interval_ends: HashMap<AllocId, IntervalEnd> = HashMap::new();
        let mut last_use_per_input: SmallVec<[Option<OperationIdx>; 8]> = SmallVec::new();

        for bb_id in program.basic_blocks.iter_idx() {
            self.compute_block_intervals(program, bb_id, &mut interval_ends);

            self.build_input_intervals(
                program,
                local_to_input_origins,
                bb_id,
                predecessors.of(bb_id),
                &mut last_use_per_input,
            );
        }

        for alloc in self.allocations.iter_mut() {
            alloc.intervals.sort();
            alloc.intervals.dedup();
        }
    }

    fn compute_block_intervals(
        &mut self,
        program: &EthIRProgram,
        bb_id: BasicBlockId,
        interval_ends: &mut HashMap<AllocId, IntervalEnd>,
    ) {
        let block = &program.basic_blocks[bb_id];
        interval_ends.clear();
        for alloc_id in self.block_exit_liveness[bb_id].iter() {
            interval_ends.insert(alloc_id, IntervalEnd::LiveOut);
        }

        for op_idx in block.operations.iter().rev() {
            let op = program.operations[op_idx];

            match op {
                Operation::StaticAllocZeroed(data) | Operation::StaticAllocAnyBytes(data) => {
                    if let Some(alloc_id) = self.local_to_alloc.get(data.ptr_out)
                        && let Some(end) = interval_ends.remove(alloc_id)
                    {
                        self.allocations[*alloc_id]
                            .intervals
                            .push((bb_id, Interval { start: IntervalStart::At(op_idx), end }));
                    }
                }
                Operation::DynamicAllocZeroed(data) | Operation::DynamicAllocAnyBytes(data) => {
                    let [ptr] = data.outs;
                    if let Some(alloc_id) = self.local_to_alloc.get(ptr)
                        && let Some(end) = interval_ends.remove(alloc_id)
                    {
                        self.allocations[*alloc_id]
                            .intervals
                            .push((bb_id, Interval { start: IntervalStart::At(op_idx), end }));
                    }
                }
                _ => {}
            }

            for input in op.inputs(program) {
                let Some(&alloc_id) = self.local_to_alloc.get(*input) else { continue };
                if !self.allocations[alloc_id].escapes {
                    interval_ends.entry(alloc_id).or_insert(IntervalEnd::At(op_idx));
                }
            }
        }

        for (alloc_id, end) in interval_ends.drain() {
            self.allocations[alloc_id]
                .intervals
                .push((bb_id, Interval { start: IntervalStart::LiveIn, end }));
        }
    }

    fn build_input_intervals(
        &mut self,
        program: &EthIRProgram,
        local_to_input_origins: &HashMap<LocalId, BlockInputOrigins>,
        bb_id: BasicBlockId,
        predecessors: &[BasicBlockId],
        last_use_per_input: &mut SmallVec<[Option<OperationIdx>; 8]>,
    ) {
        let block = &program.basic_blocks[bb_id];
        last_use_per_input.clear();
        last_use_per_input.resize(block.inputs.len() as usize, None);

        for op_idx in block.operations.iter().rev() {
            for input in program.operations[op_idx].inputs(program) {
                let Some(origins) = local_to_input_origins.get(input) else { continue };
                for &origin in origins {
                    debug_assert_eq!(origin.block, bb_id);
                    if last_use_per_input[origin.input_idx as usize].is_none() {
                        last_use_per_input[origin.input_idx as usize] = Some(op_idx);
                    }
                }
            }
        }

        for (pos, end_op) in last_use_per_input.iter().enumerate() {
            let Some(end_op) = *end_op else { continue };

            for pred_id in predecessors {
                let Some(&alloc_id) =
                    self.local_to_alloc.get(program.block(*pred_id).outputs()[pos])
                else {
                    continue;
                };
                // Skip escaping allocs and those already handled by compute_block_intervals.
                if self.allocations[alloc_id].escapes
                    || self.block_exit_liveness[bb_id].contains(alloc_id)
                {
                    continue;
                }

                self.allocations[alloc_id].intervals.push((
                    bb_id,
                    Interval { start: IntervalStart::LiveIn, end: IntervalEnd::At(end_op) },
                ));
            }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlockInput {
    block: BasicBlockId,
    input_idx: u32,
}
type BlockInputOrigins = SmallVec<[BlockInput; 1]>;

fn propagate_block_input_origins(
    program: &EthIRProgram,
    def_use: &DefUse,
) -> HashMap<LocalId, BlockInputOrigins> {
    let mut local_to_input_origins: HashMap<LocalId, BlockInputOrigins> = HashMap::new();
    let mut worklist: Vec<(LocalId, BlockInput)> = Vec::new();

    for block in program.blocks() {
        let bb_id = block.id();
        for (pos, &input) in (0u32..).zip(block.inputs()) {
            let input_site = BlockInput { block: bb_id, input_idx: pos };
            local_to_input_origins.entry(input).or_default().push(input_site);
            worklist.push((input, input_site));
        }
    }

    while let Some((local, input_site)) = worklist.pop() {
        for use_loc in def_use.uses_of(local) {
            let UseKind::Operation(op_idx) = use_loc.kind else { continue };
            let op = program.operations[op_idx];
            if !can_derive_pointer(op) {
                continue;
            }
            for output in op.outputs(program) {
                let sites = local_to_input_origins.entry(*output).or_default();
                if sites.contains(&input_site) {
                    continue;
                }
                sites.push(input_site);
                worklist.push((*output, input_site));
            }
        }
    }

    local_to_input_origins
}

fn populate_input_live_flags(
    program: &EthIRProgram,
    local_to_input_origins: &HashMap<LocalId, BlockInputOrigins>,
    bb_id: BasicBlockId,
    input_live_flags: &mut SmallVec<[bool; 8]>,
) {
    let block = &program.basic_blocks[bb_id];
    input_live_flags.clear();
    input_live_flags.resize(block.inputs.len() as usize, false);

    let mut mark_input_origins_live = |local: &LocalId| {
        let Some(origins) = local_to_input_origins.get(local) else { return };
        for origin in origins {
            debug_assert_eq!(origin.block, bb_id);
            input_live_flags[origin.input_idx as usize] = true;
        }
    };

    for output in &program.locals[block.outputs] {
        mark_input_origins_live(output);
    }

    for op_idx in block.operations.iter() {
        for input in program.operations[op_idx].inputs(program) {
            mark_input_origins_live(input);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            IntervalEnd::LiveOut,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(1),
            IntervalStart::LiveIn,
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
            IntervalEnd::LiveOut,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(1),
            IntervalStart::LiveIn,
            IntervalEnd::LiveOut,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(2),
            IntervalStart::LiveIn,
            IntervalEnd::LiveOut,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(3),
            IntervalStart::LiveIn,
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
            IntervalEnd::LiveOut,
        );
        assert_has_interval(
            alloc,
            BasicBlockId::new(1),
            IntervalStart::LiveIn,
            IntervalEnd::LiveOut,
        );
    }

    #[test]
    fn escaping_alloc_no_intervals() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    sz = const 32
                    mstore256 buf sz
                    return buf sz
                }
            "#,
            EmitConfig::init_only(),
        );
        let store = AnalysesStore::default();
        let liveness = store.allocation_liveness(&ir);
        let alloc = get_alloc(&liveness, 0);
        assert!(alloc.escapes);
        assert!(alloc.intervals.is_empty());
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
        assert_eq!(alloc.intervals.len(), 1);
        assert_has_interval(
            alloc,
            BasicBlockId::new(0),
            IntervalStart::At(op_idx_in_block(&ir, BasicBlockId::new(0), 0)), // salloc
            IntervalEnd::At(op_idx_in_block(&ir, BasicBlockId::new(0), 3)),   // mload256
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
        assert!(get_alloc(&liveness, 0).intervals.is_empty());
        assert!(get_alloc(&liveness, 1).intervals.is_empty());
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
