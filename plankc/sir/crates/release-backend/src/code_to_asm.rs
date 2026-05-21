use crate::mark_map::{IndexableMarkSpan, MarkMap};
use alloy_primitives::U256;
use plank_core::{DenseIndexSet, IncIterable};
use sir_assembler::{AsmReference, Assembler, MarkId, MarkReference, op};
use sir_data::{
    BasicBlockId, ControlView, DataId, EthIRProgram, FunctionId, Operation, OperationIdx,
    operation::{IRMemoryIOByteSize, MemoryLoadData, MemoryStoreData, StaticAllocData},
};
use sir_stack_scheduling::{ScheduledOps, stack::StackOps};
use sir_static_memory_allocator as static_mem;
use smallvec::SmallVec;

const ASM_BYTES_CAPACITY: usize = 20_000;
const ASM_SECTIONS_CAPACITY: usize = 2048;
const ICALL_RETURN_MARKS_INLINE_CAPACITY: usize = 16;

type ICallReturnMarks = SmallVec<[(OperationIdx, MarkId); ICALL_RETURN_MARKS_INLINE_CAPACITY]>;

pub(crate) trait CodegenState {
    const ALLOW_INITCODE_INTROSPECTION: bool;

    fn layout(&self) -> &static_mem::Layout;
    fn bb_marks(&self) -> IndexableMarkSpan<BasicBlockId>;
    fn mark_to_ref(&self, marks: &MarkMap, mark: MarkId) -> MarkReference;
    fn data_to_ref(&mut self, marks: &MarkMap, data: DataId) -> MarkReference;
}

pub(crate) struct CodeToAsmEmitter<'a> {
    pub mark_map: MarkMap,
    pub asm: Assembler,
    pub ir: &'a EthIRProgram,
    ops: &'a ScheduledOps,
    visited_bbs: DenseIndexSet<BasicBlockId>,
    basic_blocks_worklist: Vec<BasicBlockId>,
}

impl<'a> CodeToAsmEmitter<'a> {
    pub fn new(
        ir: &'a EthIRProgram,
        ops: &'a ScheduledOps,
        mut visited_bbs: DenseIndexSet<BasicBlockId>,
        mut basic_blocks_worklist: Vec<BasicBlockId>,
    ) -> Self {
        let mark_map = MarkMap::new(ir);
        let asm = Assembler::with_capacity(ASM_BYTES_CAPACITY, ASM_SECTIONS_CAPACITY);

        // Extra clear just to be safe.
        visited_bbs.clear();
        basic_blocks_worklist.clear();

        Self { ir, ops, mark_map, visited_bbs, basic_blocks_worklist, asm }
    }

    pub fn alloc_bb_marks(&mut self) -> IndexableMarkSpan<BasicBlockId> {
        MarkMap::alloc_map(&mut self.mark_map.next_mark_id, self.ir.basic_blocks.len())
    }

    fn reset_for_entrypoint(&mut self) {
        self.basic_blocks_worklist.clear();
        self.visited_bbs.clear();
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

        let mut icall_return_marks = ICallReturnMarks::new();

        while let Some(bb_id) = self.basic_blocks_worklist.pop() {
            let jumpdest_mark = state.bb_marks().get(bb_id);
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
                        icall_return_marks.push((op_idx, return_dest_mark));

                        let mark_ref = state.mark_to_ref(&self.mark_map, return_dest_mark);
                        self.asm.push_reference(AsmReference::pushed(mark_ref));
                    }
                    StackOps::Op(op_idx) => {
                        self.emit_op(state, &mut icall_return_marks, op_idx);
                    }
                }
            }

            let block = self.ir.block(bb_id);

            match block.control() {
                ControlView::LastOpTerminates => { /* scheduled and handled above */ }
                ControlView::InternalReturn => {
                    self.asm.push_op_byte(op::JUMP);
                }
                ControlView::ContinuesTo(to) => {
                    let to_mark = state.bb_marks().get(to);
                    let to_ref = state.mark_to_ref(&self.mark_map, to_mark);
                    self.asm.push_reference(AsmReference::pushed(to_ref));
                    self.asm.push_op_byte(op::JUMP);
                }
                ControlView::Branches { condition: _, non_zero_target, zero_target } => {
                    let non_zero_mark = state.bb_marks().get(non_zero_target);
                    let non_zero_ref = state.mark_to_ref(&self.mark_map, non_zero_mark);
                    let zero_mark = state.bb_marks().get(zero_target);
                    let zero_ref = state.mark_to_ref(&self.mark_map, zero_mark);

                    self.asm.push_reference(AsmReference::pushed(non_zero_ref));
                    self.asm.push_op_byte(op::JUMPI);
                    self.asm.push_reference(AsmReference::pushed(zero_ref));
                    self.asm.push_op_byte(op::JUMP);
                }
                ControlView::Switch(switch) => {
                    let switch_store_addr =
                        state.layout().switch_store.expect("missing switch allocation").get();

                    self.asm.push_minimal_u32(switch_store_addr);
                    self.asm.push_op_byte(op::MSTORE);

                    for (value, to) in switch.cases() {
                        let to_mark = state.bb_marks().get(to);
                        let to_ref = state.mark_to_ref(&self.mark_map, to_mark);
                        self.asm.push_minimal_u32(switch_store_addr);
                        self.asm.push_op_byte(op::MLOAD);
                        self.asm.push_minimal_u256(value);
                        self.asm.push_op_byte(op::EQ);
                        self.asm.push_reference(AsmReference::pushed(to_ref));
                        self.asm.push_op_byte(op::JUMPI);
                    }

                    if let Some(to) = switch.fallback() {
                        let to_mark = state.bb_marks().get(to);
                        let to_ref = state.mark_to_ref(&self.mark_map, to_mark);
                        self.asm.push_reference(AsmReference::pushed(to_ref));
                        self.asm.push_op_byte(op::JUMP);
                    }
                }
            }

            for succ in block.successors() {
                self.enqueue_bb(succ);
            }
        }
    }

    fn emit_op<State: CodegenState>(
        &mut self,
        state: &mut State,
        icall_return_marks: &mut ICallReturnMarks,
        op_idx: OperationIdx,
    ) {
        let op = self.ir.operations[op_idx];
        if let Some(evm_op) = op.kind().as_literal_evm_op() {
            self.asm.push_op_byte(evm_op);
            return;
        }

        match op {
            Operation::InternalCall(args) => {
                self.emit_icall(state, icall_return_marks, op_idx, args.function)
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
                let r#ref = state.data_to_ref(&self.mark_map, args.segment_id);
                self.asm.push_reference(AsmReference::pushed(r#ref));
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
            Operation::SetCopy(_) | Operation::Noop(()) => {
                // interpreted as a stack operation "setcopy" would pop and push back the top
                // making it a noop.
            }
            _ => unreachable!("op neither 'special' or literal EVM: {:?}", op.kind()),
        }
    }

    fn emit_icall(
        &mut self,
        state: &impl CodegenState,
        icall_return_marks: &mut ICallReturnMarks,
        op_idx: OperationIdx,
        function: FunctionId,
    ) {
        let function_entry_ref = {
            let call_entry_bb = self.ir.function(function).entry().id();
            self.enqueue_bb(call_entry_bb);
            let bb_entry_mark = state.bb_marks().get(call_entry_bb);
            state.mark_to_ref(&self.mark_map, bb_entry_mark)
        };
        let call_return_dest = {
            let (i, mark) = icall_return_marks
                .iter()
                .enumerate()
                .find_map(|(i, &(ret_dest_op_idx, mark))| {
                    (ret_dest_op_idx == op_idx).then_some((i, mark))
                })
                .expect("return dest not emitted *before* icall");
            icall_return_marks.swap_remove(i);
            mark
        };

        self.asm.push_reference(AsmReference::pushed(function_entry_ref));
        self.asm.push_op_byte(op::JUMP);
        self.asm.push_mark(call_return_dest);
        self.asm.push_op_byte(op::JUMPDEST);
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
        let layout = state.layout();
        let addr = layout.alloc_start[&args.alloc_id];
        self.asm.push_minimal_u32(addr.get()); //       [alloc_ptr]
        let needs_zeroing = layout.alloc_needs_zeroing.contains(&args.alloc_id);
        if needs_zeroing {
            self.asm.push_minimal_u32(args.size); //    [alloc_ptr, alloc_size]
            self.asm.push_op_byte(op::CALLDATASIZE); // [alloc_ptr, alloc_size, cd_size]
            self.asm.push_op_byte(op::DUP3); //         [alloc_ptr, alloc_size, cd_size, alloc_ptr]
            self.asm.push_op_byte(op::CALLDATACOPY); // [alloc_ptr]
        }
    }

    fn emit_memory_load(&mut self, data: MemoryLoadData) {
        match data.size {
            IRMemoryIOByteSize::B32 => self.asm.push_op_byte(op::MLOAD),
            non_native_load_size => {
                self.asm.push_op_byte(op::MLOAD);
                self.asm.push_minimal_u32(256 - u32::from(non_native_load_size.bits()));
                self.asm.push_op_byte(op::SHR);
            }
        }
    }

    fn emit_memory_store(&mut self, data: MemoryStoreData) {
        use IRMemoryIOByteSize as MemSize;

        match data.size {
            MemSize::B1 => self.asm.push_op_byte(op::MSTORE8),
            MemSize::B32 => self.asm.push_op_byte(op::MSTORE),
            non_native_size => {
                let bits = u32::from(non_native_size.bits());
                // Stack shown deepest => highest;  input: [value, ptr]
                self.asm.push_op_byte(op::SWAP1); //       [ptr, value]
                self.asm.push_minimal_u32(256 - bits); //  [ptr, value, value_shift]
                self.asm.push_op_byte(op::SHL); //         [ptr, shifted_value]
                self.asm.push_op_byte(op::DUP2); //        [ptr, shifted_value, ptr]
                self.asm.push_op_byte(op::MLOAD); //       [ptr, shifted_value, full_word]

                if (non_native_size as u8) >= 28 {
                    let mask = U256::ONE.wrapping_shl(256 - bits as usize).wrapping_sub(U256::ONE);
                    self.asm.push_minimal_u256(mask); //   [ptr, shifted_value, full_word, preserved_word_mask]
                    self.asm.push_op_byte(op::AND); //     [ptr, shifted_value, cleaned_word]
                } else {
                    self.asm.push_minimal_u32(bits); //    [ptr, shifted_value, full_word, bits]
                    self.asm.push_op_byte(op::SHL); //     [ptr, shifted_value, shifted_word]
                    self.asm.push_minimal_u32(bits); //    [ptr, shifted_value, shifted_word, bits]
                    self.asm.push_op_byte(op::SHR); //     [ptr, shifted_value, cleaned_word]
                }

                self.asm.push_op_byte(op::XOR); //         [ptr, new_word]
                self.asm.push_op_byte(op::SWAP1); //       [new_word, ptr]
                self.asm.push_op_byte(op::MSTORE); //      []
            }
        }
    }
}
