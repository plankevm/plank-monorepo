use hashbrown::HashMap;
use smallvec::{SmallVec, smallvec};

use crate::analyses::{AnalysesStore, DefUse, Predecessors, UseKind, dfs_postorder};
use plank_core::{DenseIndexSet, Idx};
use sir_data::{
    BasicBlock, BasicBlockId, Control, EthIRProgram, IndexVec, LocalId, Operation, OperationIdx,
    StaticAllocId, index_vec, newtype_index,
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

#[derive(Debug, Clone)]
pub struct AllocationLiveness {
    pub allocations: IndexVec<AllocId, AllocData>,
    pub local_to_alloc: IndexVec<LocalId, Option<AllocId>>,
}

/// Only tracks non-escaping allocations for now.
pub fn compute_allocation_liveness(program: &EthIRProgram, def_use: &DefUse) -> AllocationLiveness {
    let mut liveness = discover_allocations(program, def_use);
    if liveness.allocations.is_empty() {
        return liveness;
    }

    let local_to_input_origins = propagate_block_input_origins(program, def_use);

    let mut blocks_postorder = Vec::new();
    let mut visited = DenseIndexSet::new();
    for func in program.functions_iter() {
        dfs_postorder(program, func.entry().id(), &mut visited, &mut blocks_postorder);
    }

    let store = AnalysesStore::default();
    let predecessors = store.predecessors(program);

    let block_exit_alloc_liveness = compute_block_exit_alloc_liveness(
        program,
        &liveness.allocations,
        &liveness.local_to_alloc,
        &local_to_input_origins,
        &predecessors,
        &blocks_postorder,
    );

    populate_allocation_intervals(
        program,
        &mut liveness,
        &local_to_input_origins,
        &predecessors,
        &block_exit_alloc_liveness,
    );

    liveness
}

fn discover_allocations(program: &EthIRProgram, def_use: &DefUse) -> AllocationLiveness {
    let mut allocations: IndexVec<AllocId, AllocData> = IndexVec::new();
    let mut local_to_alloc = index_vec![None; program.next_free_local_id.get() as usize];

    for block in program.blocks() {
        let bb_id = block.id();
        for op in block.operations() {
            let (alloc_id, base_ptr) = match op.op() {
                Operation::StaticAllocZeroed(data) | Operation::StaticAllocAnyBytes(data) => {
                    let alloc_id = allocations.push(AllocData {
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
                    let alloc_id = allocations.push(AllocData {
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
            local_to_alloc[base_ptr] = Some(alloc_id);
        }
    }

    for alloc_id in allocations.indices() {
        let base_ptr = allocations[alloc_id].base_ptr;
        propagate_pointers_and_mark_escapes(
            program,
            def_use,
            &mut allocations,
            &mut local_to_alloc,
            alloc_id,
            base_ptr,
        );
    }

    AllocationLiveness { allocations, local_to_alloc }
}

fn propagate_pointers_and_mark_escapes(
    program: &EthIRProgram,
    def_use: &DefUse,
    allocations: &mut IndexVec<AllocId, AllocData>,
    local_to_alloc: &mut IndexVec<LocalId, Option<AllocId>>,
    alloc_id: AllocId,
    ptr_local: LocalId,
) {
    let mut worklist = vec![ptr_local];

    while let Some(local) = worklist.pop() {
        for use_loc in def_use.uses_of(local) {
            match use_loc.kind {
                UseKind::Operation(op_idx) => {
                    let op = program.operations[op_idx];

                    if can_derive_pointer(op) {
                        for &out in op.outputs(program) {
                            match local_to_alloc[out] {
                                None => {
                                    local_to_alloc[out] = Some(alloc_id);
                                    worklist.push(out);
                                }
                                Some(existing) if existing != alloc_id => {
                                    allocations[alloc_id].escapes = true;
                                    allocations[existing].escapes = true;
                                }
                                Some(_) => {}
                            }
                        }
                    }

                    match op {
                        Operation::Return(_)
                        | Operation::Revert(_)
                        | Operation::Log0(_)
                        | Operation::Log1(_)
                        | Operation::Log2(_)
                        | Operation::Log3(_)
                        | Operation::Log4(_)
                        | Operation::Call(_)
                        | Operation::CallCode(_)
                        | Operation::DelegateCall(_)
                        | Operation::StaticCall(_)
                        | Operation::Create(_)
                        | Operation::Create2(_)
                        | Operation::InternalCall(_) => {
                            allocations[alloc_id].escapes = true;
                        }
                        Operation::MemoryStore(data) if data.value() == local => {
                            allocations[alloc_id].escapes = true;
                        }
                        _ => {}
                    }
                }
                UseKind::BlockOutput => {
                    let block = &program.basic_blocks[use_loc.block_id];
                    if matches!(block.control, Control::InternalReturn) {
                        allocations[alloc_id].escapes = true;
                    }
                }
                UseKind::Control => {}
            }
        }
    }
}

fn can_derive_pointer(op: Operation) -> bool {
    matches!(
        op,
        Operation::Add(_)
            | Operation::Sub(_)
            | Operation::Mul(_)
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

type BlockInputOrigins = SmallVec<[(BasicBlockId, u32); 1]>;

fn propagate_block_input_origins(
    program: &EthIRProgram,
    def_use: &DefUse,
) -> HashMap<LocalId, BlockInputOrigins> {
    let mut local_to_input_origins: HashMap<LocalId, BlockInputOrigins> = HashMap::new();
    let mut worklist: Vec<(LocalId, (BasicBlockId, u32))> = Vec::new();

    for block in program.blocks() {
        let bb_id = block.id();
        for (pos, &input) in block.inputs().iter().enumerate() {
            let input_site = (bb_id, pos as u32);
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

fn compute_block_exit_alloc_liveness(
    program: &EthIRProgram,
    allocations: &IndexVec<AllocId, AllocData>,
    local_to_alloc: &IndexVec<LocalId, Option<AllocId>>,
    local_to_input_origins: &HashMap<LocalId, BlockInputOrigins>,
    predecessors: &Predecessors,
    blocks_postorder: &[BasicBlockId],
) -> IndexVec<BasicBlockId, DenseIndexSet<AllocId>> {
    let mut block_exit_alloc_liveness: IndexVec<BasicBlockId, DenseIndexSet<AllocId>> =
        index_vec![DenseIndexSet::new(); program.basic_blocks.len()];

    // Note: we recompute input_live_flags per iteration rather than precomputing and storing it.
    let mut input_live_flags = SmallVec::<[bool; 8]>::new();
    let mut changed = true;
    while changed {
        changed = false;

        for &bb_id in blocks_postorder {
            let block = &program.basic_blocks[bb_id];
            let mut exit_liveness = block_exit_alloc_liveness[bb_id].clone();

            update_block_liveness(program, allocations, local_to_alloc, block, &mut exit_liveness);

            populate_input_live_flags(
                program,
                local_to_input_origins,
                bb_id,
                &mut input_live_flags,
            );

            changed |= propagate_alloc_liveness_to_predecessors(
                program,
                allocations,
                local_to_alloc,
                &mut block_exit_alloc_liveness,
                predecessors.of(bb_id),
                &exit_liveness,
                &input_live_flags,
            );
        }
    }

    block_exit_alloc_liveness
}

fn update_block_liveness(
    program: &EthIRProgram,
    allocations: &IndexVec<AllocId, AllocData>,
    local_to_alloc: &IndexVec<LocalId, Option<AllocId>>,
    block: &BasicBlock,
    live_allocs: &mut DenseIndexSet<AllocId>,
) {
    for op_idx in block.operations.iter().rev() {
        let op = program.operations[op_idx];

        match op {
            Operation::StaticAllocZeroed(data) | Operation::StaticAllocAnyBytes(data) => {
                if let Some(alloc_id) = local_to_alloc[data.ptr_out] {
                    live_allocs.remove(alloc_id);
                }
            }
            Operation::DynamicAllocZeroed(data) | Operation::DynamicAllocAnyBytes(data) => {
                if let Some(alloc_id) = local_to_alloc[data.outs[0]] {
                    live_allocs.remove(alloc_id);
                }
            }
            _ => {}
        }

        for input in op.inputs(program) {
            let Some(alloc_id) = local_to_alloc[*input] else { continue };
            if !allocations[alloc_id].escapes {
                live_allocs.add(alloc_id);
            }
        }
    }
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
        for &(origin_block, pos) in origins {
            debug_assert_eq!(origin_block, bb_id);
            input_live_flags[pos as usize] = true;
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

fn propagate_alloc_liveness_to_predecessors(
    program: &EthIRProgram,
    allocations: &IndexVec<AllocId, AllocData>,
    local_to_alloc: &IndexVec<LocalId, Option<AllocId>>,
    block_exit_alloc_liveness: &mut IndexVec<BasicBlockId, DenseIndexSet<AllocId>>,
    predecessors: &[BasicBlockId],
    entry_liveness: &DenseIndexSet<AllocId>,
    input_live_flags: &[bool],
) -> bool {
    let mut changed = false;

    for &pred_id in predecessors {
        let pred_exit = &mut block_exit_alloc_liveness[pred_id];

        for alloc_id in entry_liveness.iter() {
            if pred_exit.add(alloc_id) {
                changed = true;
            }
        }

        let pred_outputs = program.block(pred_id).outputs();
        for (pos, &live) in input_live_flags.iter().enumerate() {
            if !live {
                continue;
            }
            if let Some(alloc_id) = local_to_alloc[pred_outputs[pos]]
                && !allocations[alloc_id].escapes
                && pred_exit.add(alloc_id)
            {
                changed = true;
            }
        }
    }

    changed
}

fn populate_allocation_intervals(
    program: &EthIRProgram,
    liveness: &mut AllocationLiveness,
    local_to_input_origins: &HashMap<LocalId, BlockInputOrigins>,
    predecessors: &Predecessors,
    block_exit_alloc_liveness: &IndexVec<BasicBlockId, DenseIndexSet<AllocId>>,
) {
    let AllocationLiveness { allocations, local_to_alloc } = liveness;
    let mut interval_ends: HashMap<AllocId, IntervalEnd> = HashMap::new();

    for bb_id in program.basic_blocks.indices() {
        compute_block_intervals(
            program,
            allocations,
            local_to_alloc,
            bb_id,
            &block_exit_alloc_liveness[bb_id],
            &mut interval_ends,
        );

        build_input_intervals(
            program,
            allocations,
            local_to_alloc,
            local_to_input_origins,
            bb_id,
            &block_exit_alloc_liveness[bb_id],
            predecessors.of(bb_id),
        );
    }

    for alloc in allocations.iter_mut() {
        alloc.intervals.sort();
        alloc.intervals.dedup();
    }
}

fn compute_block_intervals(
    program: &EthIRProgram,
    allocations: &mut IndexVec<AllocId, AllocData>,
    local_to_alloc: &IndexVec<LocalId, Option<AllocId>>,
    bb_id: BasicBlockId,
    exit_alloc_liveness: &DenseIndexSet<AllocId>,
    interval_ends: &mut HashMap<AllocId, IntervalEnd>,
) {
    let block = &program.basic_blocks[bb_id];
    interval_ends.clear();
    for alloc_id in exit_alloc_liveness.iter() {
        interval_ends.insert(alloc_id, IntervalEnd::LiveOut);
    }

    for op_idx in block.operations.iter().rev() {
        let op = program.operations[op_idx];

        match op {
            Operation::StaticAllocZeroed(data) | Operation::StaticAllocAnyBytes(data) => {
                if let Some(alloc_id) = local_to_alloc[data.ptr_out]
                    && let Some(end) = interval_ends.remove(&alloc_id)
                {
                    allocations[alloc_id]
                        .intervals
                        .push((bb_id, Interval { start: IntervalStart::At(op_idx), end }));
                }
            }
            Operation::DynamicAllocZeroed(data) | Operation::DynamicAllocAnyBytes(data) => {
                if let Some(alloc_id) = local_to_alloc[data.outs[0]]
                    && let Some(end) = interval_ends.remove(&alloc_id)
                {
                    allocations[alloc_id]
                        .intervals
                        .push((bb_id, Interval { start: IntervalStart::At(op_idx), end }));
                }
            }
            _ => {}
        }

        for input in op.inputs(program) {
            let Some(alloc_id) = local_to_alloc[*input] else { continue };
            if !allocations[alloc_id].escapes {
                interval_ends.entry(alloc_id).or_insert(IntervalEnd::At(op_idx));
            }
        }
    }

    for (alloc_id, end) in interval_ends.drain() {
        allocations[alloc_id]
            .intervals
            .push((bb_id, Interval { start: IntervalStart::LiveIn, end }));
    }
}

fn build_input_intervals(
    program: &EthIRProgram,
    allocations: &mut IndexVec<AllocId, AllocData>,
    local_to_alloc: &IndexVec<LocalId, Option<AllocId>>,
    local_to_input_origins: &HashMap<LocalId, BlockInputOrigins>,
    bb_id: BasicBlockId,
    exit_alloc_liveness: &DenseIndexSet<AllocId>,
    predecessors: &[BasicBlockId],
) {
    let block = &program.basic_blocks[bb_id];
    let mut last_use_per_input: SmallVec<[Option<OperationIdx>; 8]> =
        smallvec![None; block.inputs.len() as usize];

    for op_idx in block.operations.iter().rev() {
        for input in program.operations[op_idx].inputs(program) {
            let Some(origins) = local_to_input_origins.get(input) else { continue };
            for &(origin_block, pos) in origins {
                debug_assert_eq!(origin_block, bb_id);
                if last_use_per_input[pos as usize].is_none() {
                    last_use_per_input[pos as usize] = Some(op_idx);
                }
            }
        }
    }

    for (pos, end_op) in last_use_per_input.iter().enumerate() {
        let Some(&end_op) = end_op.as_ref() else { continue };

        for pred_id in predecessors {
            let Some(alloc_id) = local_to_alloc[program.block(*pred_id).outputs()[pos]] else {
                continue;
            };
            if allocations[alloc_id].escapes {
                continue;
            }
            // Already handled by compute_block_intervals.
            if exit_alloc_liveness.contains(alloc_id) {
                continue;
            }

            allocations[alloc_id].intervals.push((
                bb_id,
                Interval { start: IntervalStart::LiveIn, end: IntervalEnd::At(end_op) },
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sir_parser::{EmitConfig, parse_or_panic};

    fn compute_test_liveness(ir: &EthIRProgram) -> AllocationLiveness {
        let store = AnalysesStore::default();
        let def_use = store.def_use(ir);
        compute_allocation_liveness(ir, &def_use)
    }

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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
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
        let liveness = compute_test_liveness(&ir);
        assert_eq!(liveness.allocations.len(), 2);
        assert!(get_alloc(&liveness, 0).escapes, "pointer stored as value should escape");
        assert!(!get_alloc(&liveness, 1).escapes, "pointer used as address should not escape");
    }
}
