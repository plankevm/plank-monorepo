use crate::mark_map::MarkMap;
use plank_core::IncIterable;
use sir_assembler::{AsmReference, Assembler, MarkId, MarkReference, op};
use sir_data::{
    BasicBlockId, DataId, DenseIndexSet, EthIRProgram, FunctionId, Operation, OperationIdx, Span,
    operation::{
        IRMemoryIOByteSize, InlineOperands, MemoryLoadData, MemoryStoreData, StaticAllocData,
    },
};
use sir_stack_scheduling::{ScheduledOps, stack::StackOps};
use sir_static_memory_allocator as static_mem;
use smallvec::SmallVec;

const BB_WORKLIST_CAPACITY: usize = 128;
const ASM_BYTES_CAPACITY: usize = 20_000;
const ASM_SECTIONS_CAPACITY: usize = 2048;
const ICALL_RETURNS_INLINE_CAPACITY: usize = 16;

type ICallReturns = SmallVec<[(OperationIdx, MarkId); ICALL_RETURNS_INLINE_CAPACITY]>;

pub(crate) trait CodegenState {
    const ALLOW_INITCODE_INTROSPECTION: bool;

    fn layout(&self) -> &static_mem::Layout;
    fn bb_to_jumpdest_mark(&self, bb: BasicBlockId) -> MarkId;
    fn mark_to_ref(&self, map: &MarkMap, mark: MarkId) -> MarkReference;
}

pub(crate) struct CodeToAsmEmitter<'a> {
    pub mark_map: MarkMap,
    pub asm: Assembler,
    ir: &'a EthIRProgram,
    ops: &'a ScheduledOps,
    visited_bbs: DenseIndexSet<BasicBlockId>,
    basic_blocks_worklist: Vec<BasicBlockId>,
    runtime_datas: DenseIndexSet<DataId>,
}

impl<'a> CodeToAsmEmitter<'a> {
    pub fn new(ir: &'a EthIRProgram, ops: &'a ScheduledOps) -> Self {
        let mut visited_bbs = DenseIndexSet::with_capacity_in_bits(ir.basic_blocks.len());
        let mut basic_blocks_worklist = Vec::with_capacity(BB_WORKLIST_CAPACITY);

        let runtime_datas = match ir.main_entry {
            Some(runtime_entrypoint) => {
                let mut runtime_datas =
                    DenseIndexSet::with_capacity_in_bits(ir.data_segments.len());
                Self::collect_runtime_datas(
                    ir,
                    &mut visited_bbs,
                    &mut basic_blocks_worklist,
                    &mut runtime_datas,
                    runtime_entrypoint,
                );
                runtime_datas
            }
            None => DenseIndexSet::new(),
        };

        let mark_map = MarkMap::new(ir);
        let asm = Assembler::with_capacity(ASM_BYTES_CAPACITY, ASM_SECTIONS_CAPACITY);

        Self { ir, ops, mark_map, visited_bbs, basic_blocks_worklist, asm, runtime_datas }
    }

    pub fn asm_mut(&mut self) -> &mut Assembler {
        &mut self.asm
    }

    pub fn mark_map(&self) -> &MarkMap {
        &self.mark_map
    }

    pub fn alloc_bb_marks(&mut self) -> Span<MarkId> {
        MarkMap::alloc_id_span(&mut self.mark_map.next_mark_id, self.ir.basic_blocks.len())
    }

    fn reset_for_entrypoint(&mut self) {
        self.basic_blocks_worklist.clear();
        self.visited_bbs.clear();
    }

    fn collect_runtime_datas(
        ir: &'a EthIRProgram,
        visited_bbs: &mut DenseIndexSet<BasicBlockId>,
        basic_blocks_worklist: &mut Vec<BasicBlockId>,
        runtime_datas: &mut DenseIndexSet<DataId>,
        runtime_entrypoint: FunctionId,
    ) {
        let entry_bb = ir.function(runtime_entrypoint).entry().id();
        visited_bbs.add(entry_bb);
        basic_blocks_worklist.push(entry_bb);
        while let Some(bb_id) = basic_blocks_worklist.pop() {
            let block = ir.block(bb_id);
            for op in block.operations() {
                match op.op() {
                    Operation::SetDataOffset(set_data) => {
                        runtime_datas.add(set_data.segment_id);
                    }
                    Operation::InternalCall(icall) => {
                        let fn_entry = ir.functions[icall.function].entry();
                        if visited_bbs.add(fn_entry) {
                            basic_blocks_worklist.push(fn_entry);
                        }
                    }
                    _ => {}
                }
            }
            for succ in block.successors() {
                if visited_bbs.add(succ) {
                    basic_blocks_worklist.push(succ);
                }
            }
        }
    }

    fn enqueue_bb(&mut self, bb: BasicBlockId) -> bool {
        if self.visited_bbs.add(bb) {
            self.basic_blocks_worklist.push(bb);
            true
        } else {
            false
        }
    }

    pub fn emit_from_entrypoint(&mut self, state: &mut impl CodegenState, entrypoint: FunctionId) {
        self.reset_for_entrypoint();

        if let Some(free_pointer) = state.layout().dyn_free_pointer {
            self.asm.push_minimal_u32(free_pointer.start_value.get());
            self.asm.push_minimal_u32(free_pointer.store_slot.get());
            self.asm.push_op_byte(op::MSTORE);
        }

        let entry_bb = self.ir.function(entrypoint).entry().id();
        assert!(self.enqueue_bb(entry_bb));

        let mut icall_returns = ICallReturns::new();

        while let Some(bb_id) = self.basic_blocks_worklist.pop() {
            let jumpdest_mark = state.bb_to_jumpdest_mark(bb_id);
            self.asm.push_mark(jumpdest_mark);
            self.asm.push_op_byte(op::JUMPDEST);

            let bb_ops = self.ops.get(bb_id).expect("reachable block not scheduled");
            for &op in bb_ops {
                match op {
                    StackOps::Swap(depth) => self.asm.push_swap(depth),
                    StackOps::Dup(depth) => self.asm.push_dup(depth),
                    StackOps::Pop => self.asm.push_op_byte(op::POP),
                    StackOps::Exchange(n, m) => self.asm.push_exchange(n, m),
                    StackOps::Store(alloc) => {
                        let addr = state.layout().alloc_start[&alloc];
                        self.asm.push_minimal_u32(addr.get());
                        self.asm.push_op_byte(op::MSTORE);
                    }
                    StackOps::Load(alloc) => {
                        let addr = state.layout().alloc_start[&alloc];
                        self.asm.push_minimal_u32(addr.get());
                        self.asm.push_op_byte(op::MLOAD);
                    }
                    StackOps::CallRetPush(op_idx) => {
                        let return_dest_mark = self.mark_map.next_mark_id.get_and_inc();
                        icall_returns.push((op_idx, return_dest_mark));

                        let mark_ref = state.mark_to_ref(&self.mark_map, return_dest_mark);
                        self.asm.push_reference(AsmReference::pushed(mark_ref));
                    }
                    StackOps::Op(op_idx) => {
                        self.emit_op(state, &mut icall_returns, op_idx);
                    }
                }
            }
        }
    }

    fn emit_op<State: CodegenState>(
        &mut self,
        state: &mut State,
        icall_returns: &mut ICallReturns,
        op_idx: OperationIdx,
    ) {
        let op = self.ir.operations[op_idx];
        if let Some(evm_op) = op.kind().as_literal_evm_op() {
            self.asm.push_op_byte(evm_op);
            return;
        }

        match op {
            Operation::InternalCall(args) => {
                todo!("icall")
            }
            Operation::DynamicAllocZeroed(_) => self.emit_dynamic_alloc_zeroed(state),
            Operation::DynamicAllocAnyBytes(_) => self.emit_dynamic_alloc_any_bytes(state),
            Operation::AcquireFreePointer(_) => self.emit_acquire_free_pointer(state),
            Operation::StaticAllocZeroed(args) => self.emit_static_alloc(state, args),
            Operation::StaticAllocAnyBytes(args) => self.emit_static_alloc(state, args),
            Operation::MemoryLoad(data) => self.emit_memory_load(data),
            Operation::MemoryStore(data) => self.emit_memory_store(data),
            Operation::SetSmallConst(args) => self.asm.push_minimal_u32(args.value),
            Operation::SetLargeConst(args) => {
                self.asm.push_minimal_u256(self.ir.large_consts[args.value]);
            }
            Operation::SetDataOffset(args) => {
                todo!("SetDataOffset")
            }
            Operation::RuntimeStartOffset(_) => {
                assert!(
                    State::ALLOW_INITCODE_INTROSPECTION,
                    "use of `{}` when initcode introspection disallowed",
                    op.kind().mnemonic()
                );
                self.asm.push_reference(AsmReference::new_direct(self.mark_map.runcode_start));
            }
            Operation::InitEndOffset(_) => {
                assert!(
                    State::ALLOW_INITCODE_INTROSPECTION,
                    "use of `{}` when initcode introspection disallowed",
                    op.kind().mnemonic()
                );
                self.asm.push_reference(AsmReference::new_direct(self.mark_map.initcode_end));
            }
            Operation::RuntimeLength(_) => {
                let asm_ref = AsmReference::pushed(MarkReference::Delta(self.mark_map.runcode()));
                self.asm.push_reference(asm_ref);
            }
            Operation::SetCopy(_) => { /* noop in terms of effect on the stack */ }
            Operation::Noop(()) => {}
            _ => unreachable!("op neither 'special' or literal EVM: {:?}", op.kind()),
        }
    }

    fn emit_dynamic_alloc_zeroed(&mut self, state: &impl CodegenState) {
        let free_pointer =
            state.layout().dyn_free_pointer.expect("dynamic allocation without free pointer slot");
        let free_ptr_slot = free_pointer.store_slot.get();

        // Stack shown deepest => highest; input:    [alloc_size]
        self.asm.push_minimal_u32(free_ptr_slot); // [alloc_size, free_ptr_slot]
        self.asm.push_op_byte(op::MLOAD); //         [alloc_size, free_ptr]
        self.asm.push_op_byte(op::DUP2); //          [alloc_size, free_ptr, alloc_size]
        self.asm.push_op_byte(op::DUP2); //          [alloc_size, free_ptr, alloc_size, free_ptr]
        self.asm.push_op_byte(op::ADD); //           [alloc_size, free_ptr, updated_free_ptr]
        self.asm.push_minimal_u32(free_ptr_slot); // [alloc_size, free_ptr, updated_free_ptr, free_ptr_slot]
        self.asm.push_op_byte(op::MSTORE); //        [alloc_size, free_ptr]
        self.asm.push_op_byte(op::SWAP1); //         [free_ptr, alloc_size]
        self.asm.push_op_byte(op::CALLDATASIZE); //  [free_ptr, alloc_size, cd_size]
        self.asm.push_op_byte(op::DUP3); //          [free_ptr, alloc_size, cd_size, free_ptr]
        self.asm.push_op_byte(op::CALLDATACOPY); //  [free_ptr]
    }

    fn emit_dynamic_alloc_any_bytes(&mut self, state: &impl CodegenState) {
        let free_pointer =
            state.layout().dyn_free_pointer.expect("dynamic allocation without free pointer slot");
        let free_ptr_slot = free_pointer.store_slot.get();

        // Stack shown deepest => highest; input:    [alloc_size]
        self.asm.push_minimal_u32(free_ptr_slot); // [alloc_size, free_ptr_slot]
        self.asm.push_op_byte(op::MLOAD); //         [alloc_size, free_ptr]
        self.asm.push_op_byte(op::SWAP1); //         [free_ptr, alloc_size]
        self.asm.push_op_byte(op::DUP2); //          [free_ptr, alloc_size, free_ptr]
        self.asm.push_op_byte(op::ADD); //           [free_ptr, updated_free_ptr]
        self.asm.push_minimal_u32(free_ptr_slot); // [free_ptr, updated_free_ptr, free_ptr_slot]
        self.asm.push_op_byte(op::MSTORE); //        [free_ptr]
    }

    fn emit_acquire_free_pointer(&mut self, state: &impl CodegenState) {
        let free_pointer = state
            .layout()
            .dyn_free_pointer
            .expect("free pointer acquisition without free pointer slot");
        self.asm.push_minimal_u32(free_pointer.store_slot.get());
        self.asm.push_op_byte(op::MLOAD);
    }

    fn emit_static_alloc(&mut self, state: &impl CodegenState, args: StaticAllocData) {
        let addr = state.layout().alloc_start[&args.alloc_id];
        self.asm.push_minimal_u32(addr.get());
    }

    fn emit_memory_load(&mut self, data: MemoryLoadData) {
        match data.size {
            IRMemoryIOByteSize::B32 => self.asm.push_op_byte(op::MLOAD),
            non_native_load_size => {
                self.asm.push_minimal_u32(256 - u32::from(non_native_load_size.bits()));
                self.asm.push_op_byte(op::SHR);
            }
        }
    }

    fn emit_memory_store(&mut self, data: MemoryStoreData) {
        match data.size {
            IRMemoryIOByteSize::B1 => self.asm.push_op_byte(op::MSTORE8),
            IRMemoryIOByteSize::B32 => self.asm.push_op_byte(op::MSTORE),
            non_native_size => {
                let bits = u32::from(non_native_size.bits());
                // Stack states are shown deepest => highest.
                // start:                                           [value, ptr]
                self.asm.push_op_byte(op::DUP1); //                 [value, ptr, ptr]
                self.asm.push_op_byte(op::MLOAD); //                [value, ptr, full_word]
                self.asm.push_minimal_u32(bits); //                 [value, ptr, full_word, bits]
                self.asm.push_op_byte(op::SHL); //                  [value, ptr, shifted_word]
                self.asm.push_minimal_u32(bits); //                 [value, ptr, shifted_word, bits]
                self.asm.push_op_byte(op::SHR); //                  [value, ptr, cleaned_word]
                self.asm.push_op_byte(op::DUP3); //                 [value, ptr, cleaned_word, value]
                self.asm.push_minimal_u32(256 - bits); //           [value, ptr, cleaned_word, value, value_shift]
                self.asm.push_op_byte(op::SHL); //                  [value, ptr, cleaned_word, shifted_value]
                self.asm.push_op_byte(op::OR); //                   [value, ptr, updated_word]
                self.asm.push_op_byte(op::SWAP1); //                [value, updated_word, ptr]
                self.asm.push_op_byte(op::MSTORE); //               [value]
                self.asm.push_op_byte(op::POP); //                  []
            }
        }
    }
}
