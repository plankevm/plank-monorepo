use crate::{Translator, static_memory_layout::EVM_WORD_IN_BYTES};
use plank_core::Span;
use sir_assembler::{AsmReference, MarkReference, op};
use sir_data::{LocalId, OperationIdx, operation::*};

struct OpcodeTranslator<'t, 'ir> {
    translator: &'t mut Translator<'ir>,
    op_idx: OperationIdx,
}

impl<'t, 'ir> OpcodeTranslator<'t, 'ir> {
    fn emit_simple_operation(&mut self, evm_op: u8, inputs: &[LocalId], outputs: &[LocalId]) {
        for &input in inputs.iter().rev() {
            self.translator.emit_local_load(input);
        }
        self.translator.asm.push_op_byte(evm_op);
        for &output in outputs.iter() {
            self.translator.emit_local_store(output);
        }
    }

    fn emit_dynamic_memory_alloc(&mut self, size_local: LocalId, ptr_out_local: LocalId) {
        self.translator.emit_free_ptr_load(); // [free_ptr]
        self.translator.asm.push_op_byte(op::DUP1); // [free_ptr, free_ptr]
        self.translator.emit_local_load(size_local); // [size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::DUP1); // [size, size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::CALLDATASIZE); // [cdz, size, size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::DUP4); // [free_ptr, cdz, size, size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::CALLDATACOPY); // [size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::ADD); // [free_ptr', free_ptr]
        self.translator.asm.push_minimal_u32(self.translator.memory_layout.free_pointer);
        // [free_ptr_loc, free_ptr', free_ptr]
        self.translator.asm.push_op_byte(op::MSTORE); // [free_ptr]
        self.translator.emit_local_store(ptr_out_local);
    }

    fn emit_static_memory_alloc(&mut self, size: u32, ptr_out_local: LocalId) {
        self.translator.emit_free_ptr_load(); // [free_ptr]
        self.translator.asm.push_op_byte(op::DUP1); // [free_ptr, free_ptr]
        self.translator.asm.push_minimal_u32(size); // [size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::DUP1); // [size, size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::CALLDATASIZE); // [cdz, size, size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::DUP4); // [free_ptr, cdz, size, size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::CALLDATACOPY); // [size, free_ptr, free_ptr]
        self.translator.asm.push_op_byte(op::ADD); // [free_ptr', free_ptr]
        self.translator.asm.push_minimal_u32(self.translator.memory_layout.free_pointer);
        // [free_ptr_loc, free_ptr', free_ptr]
        self.translator.asm.push_op_byte(op::MSTORE); // [free_ptr]
        self.translator.emit_local_store(ptr_out_local);
    }

    fn emit_memory_load(&mut self, data: MemoryLoadData) {
        let load_size = data.size as u32;
        self.translator.emit_local_load(data.ptr);
        self.translator.asm.push_op_byte(op::MLOAD);
        self.translator.asm.push_minimal_u32(256 - load_size * 8);
        self.translator.asm.push_op_byte(op::SHR);
        self.translator.emit_local_store(data.out);
    }

    fn emit_memory_store(&mut self, data: MemoryStoreData) {
        let load_size = data.size as u32;
        let shift_to_clean_word = load_size * 8;
        self.translator.emit_local_load(data.ptr()); // [ptr]
        self.translator.asm.push_op_byte(op::DUP1); // [ptr, ptr]
        self.translator.asm.push_op_byte(op::MLOAD); // [current_word, ptr]
        self.translator.asm.push_minimal_u32(shift_to_clean_word); // [shift, current_word, ptr]
        self.translator.asm.push_op_byte(op::SHL); // [current_word << shift, ptr]
        self.translator.asm.push_minimal_u32(shift_to_clean_word); // [shift, current_word << shift, ptr]
        self.translator.asm.push_op_byte(op::SHR); // [cleaned_word, ptr]
        self.translator.emit_local_load(data.value()); // [value, cleaned_word, ptr]
        self.translator.asm.push_minimal_u32(256 - load_size * 8); // [value_shift, value, cleaned_word, ptr]
        self.translator.asm.push_op_byte(op::SHL); // [shifted_value, cleaned_word, ptr]
        self.translator.asm.push_op_byte(op::OR); // [updated_word, ptr]
        self.translator.asm.push_op_byte(op::SWAP1); // [ptr, updated_word]
        self.translator.asm.push_op_byte(op::MSTORE); // []
    }

    fn emit_set_small_const(&mut self, data: SetSmallConstData) {
        self.translator.asm.push_minimal_u32(data.value);
        self.translator.emit_local_store(data.sets);
    }

    fn emit_set_large_const(&mut self, data: SetLargeConstData) {
        self.translator.asm.push_minimal_u256(self.translator.ir.large_consts[data.value]);
        self.translator.emit_local_store(data.sets);
    }

    fn emit_set_data_offset(&mut self, data: SetDataOffsetData) {
        let data_offset_mark = self.translator.mark_map.get_data_mark(data.segment_id);
        self.translator.emit_code_offset_push(data_offset_mark);
        self.translator.emit_local_store(data.sets);
    }

    fn emit_get_immutable(&mut self, data: GetImmutableData) {
        assert!(!self.translator.translating_init_code, "getimmutable in init code");
        let size = self.translator.ir.immutables[data.immutable];
        let imm_ref = self
            .translator
            .immutable_refs
            .iter_mut()
            .find(|imm_ref| imm_ref.get_op == self.op_idx)
            .expect("getimmutable not collected as runtime reference");
        assert!(!imm_ref.emitted, "getimmutable placeholder emitted twice");
        imm_ref.emitted = true;
        let placeholder = imm_ref.placeholder;
        self.translator.asm.push_placeholder_push(size, placeholder);
        self.translator.emit_local_store(data.out);
    }

    /// The value is staged in the scratch slot so that only its low `size` bytes can be copied
    /// into each placeholder, leaving the surrounding runtime code untouched.
    fn emit_set_immutable(&mut self, data: SetImmutableData) {
        assert!(self.translator.translating_init_code, "setimmutable in runtime code");
        let size = self.translator.ir.immutables[data.immutable] as u32;
        let runtime_start = self.translator.mark_map.runtime_start;
        let scratch_slot = self.translator.memory_layout.scratch_slot;
        let copy_src = scratch_slot + EVM_WORD_IN_BYTES - size;
        let mut value_staged = false;
        for i in 0..self.translator.immutable_refs.len() {
            let imm_ref = &self.translator.immutable_refs[i];
            if imm_ref.immutable != data.immutable {
                continue;
            }
            let placeholder_offset =
                MarkReference::Delta(Span::new(runtime_start, imm_ref.placeholder));
            if !value_staged {
                value_staged = true;
                self.translator.emit_local_load(data.value()); // [value]
                self.translator.asm.push_minimal_u32(scratch_slot); // [scratch, value]
                self.translator.asm.push_op_byte(op::MSTORE); // []
            }
            self.translator.asm.push_minimal_u32(size); // [size]
            self.translator.asm.push_minimal_u32(copy_src); // [src, size]
            self.translator.emit_local_load(data.runtime_ptr()); // [runtime_ptr, src, size]
            self.translator.asm.push_reference(AsmReference::pushed(placeholder_offset)); // [offset, runtime_ptr, src, size]
            self.translator.asm.push_op_byte(op::ADD); // [dst, src, size]
            self.translator.asm.push_op_byte(op::MCOPY); // []
        }
    }

    fn emit_icall(&mut self, data: InternalCallData) {
        self.translator.memory_layout.emit_copy_for_basic_block_inputs(
            &mut self.translator.asm,
            data.get_inputs(self.translator.ir),
        );

        let return_mark = self.translator.mark_map.allocate_mark();
        let return_store_loc = self.translator.memory_layout.get_return_dest_store(data.function);
        self.translator.emit_code_offset_push(return_mark);
        self.translator.asm.push_minimal_u32(return_store_loc);
        self.translator.asm.push_op_byte(op::MSTORE);
        let func_entry_bb = self.translator.ir.function(data.function).entry().id();
        let func_entry_bb_mark = self.translator.get_bb_mark(func_entry_bb);
        self.translator.emit_code_offset_push(func_entry_bb_mark);
        self.translator.asm.push_op_byte(op::JUMP);
        self.translator.asm.push_mark(return_mark);
        self.translator.asm.push_op_byte(op::JUMPDEST);

        self.translator.memory_layout.emit_transfer_basic_block_outputs(
            &mut self.translator.asm,
            data.get_outputs(self.translator.ir),
        );

        self.translator.bbs_to_be_translated.push((data.function, func_entry_bb));
    }

    fn emit_icall_never(&mut self, data: InternalCallNeverData) {
        self.translator.memory_layout.emit_copy_for_basic_block_inputs(
            &mut self.translator.asm,
            data.get_inputs(self.translator.ir),
        );

        let func_entry_bb = self.translator.ir.function(data.function).entry().id();
        let func_entry_bb_mark = self.translator.get_bb_mark(func_entry_bb);
        self.translator.emit_code_offset_push(func_entry_bb_mark);
        self.translator.asm.push_op_byte(op::JUMP);
        self.translator.bbs_to_be_translated.push((data.function, func_entry_bb));
    }
}

pub(crate) fn translate_operation(
    translator: &mut Translator,
    op_idx: OperationIdx,
    op: Operation,
) {
    let mut t = OpcodeTranslator { translator, op_idx };
    if let Some(evm_op) = op.kind().as_literal_evm_op() {
        let ir = t.translator.ir;
        t.emit_simple_operation(evm_op, op.inputs(ir), op.outputs(ir));
        return;
    }

    match op {
        Operation::DynamicAllocZeroed(data) | Operation::DynamicAllocAnyBytes(data) => {
            t.emit_dynamic_memory_alloc(data.ins[0], data.outs[0])
        }
        Operation::AcquireFreePointer(data) => {
            t.translator.emit_free_ptr_load();
            t.translator.emit_local_store(data.outs[0]);
        }
        Operation::SetCopy(data) => {
            t.translator.emit_local_load(data.ins[0]);
            t.translator.emit_local_store(data.outs[0]);
        }
        Operation::RuntimeStartOffset(data) => {
            debug_assert!(
                t.translator.translating_init_code,
                "unexpected runtime_start_offset in run code"
            );
            t.translator
                .asm
                .push_reference(AsmReference::new_direct(t.translator.mark_map.runtime_start));
            t.translator.emit_local_store(data.outs[0]);
        }
        Operation::InitEndOffset(data) => {
            debug_assert!(
                t.translator.translating_init_code,
                "unexpected init_end_offset in run code"
            );
            t.translator
                .asm
                .push_reference(AsmReference::new_direct(t.translator.mark_map.initcode_end));
            t.translator.emit_local_store(data.outs[0]);
        }
        Operation::RuntimeLength(data) => {
            t.translator.asm.push_reference(AsmReference::new_delta(
                t.translator.mark_map.runtime_start,
                t.translator.mark_map.initcode_end,
            ));
            t.translator.emit_local_store(data.outs[0])
        }
        Operation::Noop(()) => {}
        Operation::StaticAllocZeroed(data) | Operation::StaticAllocAnyBytes(data) => {
            t.emit_static_memory_alloc(data.size, data.ptr_out)
        }
        Operation::MemoryLoad(data) => t.emit_memory_load(data),
        Operation::MemoryStore(data) => t.emit_memory_store(data),
        Operation::SetSmallConst(data) => t.emit_set_small_const(data),
        Operation::SetLargeConst(data) => t.emit_set_large_const(data),
        Operation::SetDataOffset(data) => t.emit_set_data_offset(data),
        Operation::GetImmutable(data) => t.emit_get_immutable(data),
        Operation::SetImmutable(data) => t.emit_set_immutable(data),
        Operation::InternalCall(data) => t.emit_icall(data),
        Operation::InternalCallNever(data) => t.emit_icall_never(data),
        _ => unreachable!("op neither 'special' or literal EVM: {:?}", op.kind()),
    }
}
