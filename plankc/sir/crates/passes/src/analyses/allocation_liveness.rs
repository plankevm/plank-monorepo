use crate::{DefUse, UseKind};
use sensei_core::Idx;
use sir_data::{
    BasicBlockId, Control, EthIRProgram, IndexVec, LocalId, Operation, OperationIdx, StaticAllocId,
    index_vec, newtype_index, operation::op_data::InlineOperands,
};

newtype_index! {
    pub struct AllocId;
}

#[derive(Debug, Clone, Copy)]
pub enum AllocKind {
    Static { size: u32, id: StaticAllocId },
    Dynamic { size_local: LocalId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalStart {
    LiveIn,
    At(OperationIdx),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalEnd {
    LiveOut,
    At(OperationIdx),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: IntervalStart,
    pub end: IntervalEnd,
}

#[derive(Debug, Clone)]
pub struct AllocData {
    pub def_block: BasicBlockId,
    pub def_op: OperationIdx,
    pub kind: AllocKind,
    pub escapes: bool,
    pub intervals: Vec<(BasicBlockId, Interval)>,
}

#[derive(Debug, Clone)]
pub struct AllocationLiveness {
    pub allocations: IndexVec<AllocId, AllocData>,
    pub local_to_alloc: IndexVec<LocalId, Option<AllocId>>,
}

pub fn discover_allocations(program: &EthIRProgram, def_use: &DefUse) -> AllocationLiveness {
    let mut allocations = IndexVec::new();
    let mut local_to_alloc = index_vec![None; program.next_free_local_id.get() as usize];

    for block in program.blocks() {
        let bb_id = block.id();
        for op in block.operations() {
            let (alloc_id, base_ptr) = match op.op() {
                Operation::StaticAllocZeroed(data) | Operation::StaticAllocAnyBytes(data) => {
                    let alloc_id = allocations.push(AllocData {
                        def_block: bb_id,
                        def_op: op.id(),
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
                        kind: AllocKind::Dynamic { size_local: data.ins[0] },
                        escapes: false,
                        intervals: Vec::new(),
                    });
                    (alloc_id, data.outs[0])
                }
                _ => continue,
            };
            local_to_alloc[base_ptr] = Some(alloc_id);
            propagate_pointers_and_mark_escapes(
                program,
                def_use,
                &mut allocations,
                &mut local_to_alloc,
                alloc_id,
                base_ptr,
            );
        }
    }

    AllocationLiveness { allocations, local_to_alloc }
}

fn propagate_pointers_and_mark_escapes(
    program: &EthIRProgram,
    def_use: &DefUse,
    allocations: &mut IndexVec<AllocId, AllocData>,
    local_to_alloc: &mut IndexVec<LocalId, Option<AllocId>>,
    alloc_id: AllocId,
    base_ptr: LocalId,
) {
    let mut worklist = vec![base_ptr];

    while let Some(local) = worklist.pop() {
        for use_loc in &def_use[local] {
            match use_loc.kind {
                UseKind::Operation(op_idx) => match program.operations[op_idx] {
                    Operation::Add(InlineOperands { outs: [derived], .. })
                    | Operation::Sub(InlineOperands { outs: [derived], .. })
                    | Operation::SetCopy(InlineOperands { outs: [derived], .. }) => {
                        debug_assert!(
                            local_to_alloc[derived].is_none(),
                            "local already claimed by another allocation"
                        );
                        local_to_alloc[derived] = Some(alloc_id);
                        worklist.push(derived);
                    }
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
                    _ => {}
                },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compute_def_use;
    use sir_parser::{EmitConfig, parse_or_panic};

    fn analyze(ir: &EthIRProgram) -> AllocationLiveness {
        let mut def_use = IndexVec::new();
        compute_def_use(ir, &mut def_use);
        discover_allocations(ir, &def_use)
    }

    #[test]
    fn discover_static_and_dynamic() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    sz = const 64
                    dbuf = malloc sz
                    mstore256 buf dbuf
                    return buf dbuf
                }
            "#,
            EmitConfig::init_only(),
        );
        let result = analyze(&ir);
        assert_eq!(result.allocations.len(), 2);
        assert!(matches!(
            result.allocations[AllocId::new(0)].kind,
            AllocKind::Static { size: 32, .. }
        ));
        assert!(matches!(result.allocations[AllocId::new(1)].kind, AllocKind::Dynamic { .. }));
    }

    #[test]
    fn discover_no_allocs() {
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
        let result = analyze(&ir);
        assert_eq!(result.allocations.len(), 0);
    }

    #[test]
    fn propagate_derived_pointer() {
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
        let result = analyze(&ir);
        assert_eq!(result.allocations.len(), 1);
        let alloc_id = AllocId::new(0);
        let buf_local = result.allocations[alloc_id].kind;
        assert!(matches!(buf_local, AllocKind::Static { size: 64, .. }));
        let derived_alloc = result.local_to_alloc.iter().filter(|a| **a == Some(alloc_id)).count();
        assert_eq!(derived_alloc, 2);
        assert!(!result.allocations[alloc_id].escapes);
    }

    #[test]
    fn escaping_via_return() {
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
        let result = analyze(&ir);
        assert!(result.allocations[AllocId::new(0)].escapes);
    }

    #[test]
    fn non_escaping() {
        let ir = parse_or_panic(
            r#"
            fn init:
                entry {
                    buf = salloc 32
                    v = mload256 buf
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );
        let result = analyze(&ir);
        assert!(!result.allocations[AllocId::new(0)].escapes);
    }
}
