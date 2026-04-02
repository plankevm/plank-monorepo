use sensei_core::{DenseIndexSet, Idx, IncIterable};
use sir_assembler::{AsmReference, AssembleError, Assembler, MarkId, MarkReference, op};
use sir_data::{BasicBlockId, ControlView, DataId, EthIRProgram, FunctionId, LocalId, Span};

use crate::static_memory_layout::StaticMemoryLayout;

mod operations;
mod static_memory_layout;

const ASM_BYTES_CAPACITY: usize = 20_000;
const ASM_SECTIONS_CAPACITY: usize = 512;

#[derive(Debug, Clone)]
pub struct SourceMapEntry {
    pub op_index: u32,
    pub pc: u32,
}

#[derive(Debug, Clone)]
pub enum BytecodeError {
    Assemble(AssembleError),
}

impl std::fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BytecodeError::Assemble(err) => write!(f, "assembly failed: {err:?}"),
        }
    }
}

impl std::error::Error for BytecodeError {}

pub(crate) struct MarkMap {
    init_basic_block_marks_start: MarkId,
    run_basic_block_marks_start: MarkId,
    data_marks_start: MarkId,
    runtime_start: MarkId,
    initcode_end: MarkId,
    next_mark_id: MarkId,
}

impl MarkMap {
    fn new(ir: &EthIRProgram) -> Self {
        let mut next_mark_id = MarkId::ZERO;

        let init_basic_block_marks_start = next_mark_id;
        next_mark_id += ir.basic_blocks.len() as u32;

        let run_basic_block_marks_start = next_mark_id;
        next_mark_id += ir.basic_blocks.len() as u32;

        let data_marks_start = next_mark_id;
        next_mark_id += ir.data_segments.len() as u32;

        let runtime_start = next_mark_id.get_and_inc();
        let bytecode_end = next_mark_id.get_and_inc();

        Self {
            init_basic_block_marks_start,
            run_basic_block_marks_start,
            data_marks_start,
            runtime_start,
            initcode_end: bytecode_end,
            next_mark_id,
        }
    }

    pub fn allocate_mark(&mut self) -> MarkId {
        self.next_mark_id.get_and_inc()
    }

    pub fn get_init_bb_mark(&self, bb_id: BasicBlockId) -> MarkId {
        self.init_basic_block_marks_start + bb_id.get()
    }

    pub fn get_run_bb_mark(&self, bb_id: BasicBlockId) -> MarkId {
        self.run_basic_block_marks_start + bb_id.get()
    }

    pub fn get_data_mark(&self, data_id: DataId) -> MarkId {
        self.data_marks_start + data_id.get()
    }
}

pub(crate) struct Translator<'ir> {
    pub ir: &'ir EthIRProgram,
    pub memory_layout: StaticMemoryLayout,
    pub mark_map: MarkMap,
    pub translated_bbs: DenseIndexSet<BasicBlockId>,
    pub bbs_to_be_translated: Vec<(FunctionId, BasicBlockId)>,
    pub translating_init_code: bool,
    pub asm: Assembler,
    pub source_map_marks: Vec<(MarkId, u32)>,
    pub global_op_index: u32,
}

impl<'ir> Translator<'ir> {
    fn push_source_map_mark(&mut self) {
        let op_mark = self.mark_map.allocate_mark();
        self.asm.push_mark(op_mark);
        self.source_map_marks.push((op_mark, self.global_op_index));
        self.global_op_index += 1;
    }

    pub(crate) fn emit_free_ptr_load(&mut self) {
        self.asm.push_minimal_u32(self.memory_layout.free_pointer);
        self.asm.push_op_byte(op::MLOAD);
    }

    pub(crate) fn emit_local_load(&mut self, local: LocalId) {
        self.asm.push_minimal_u32(self.memory_layout.get_local_addr(local));
        self.asm.push_op_byte(op::MLOAD);
    }

    pub(crate) fn emit_local_store(&mut self, local: LocalId) {
        self.asm.push_minimal_u32(self.memory_layout.get_local_addr(local));
        self.asm.push_op_byte(op::MSTORE);
    }

    pub(crate) fn emit_code_offset_push(&mut self, offset_mark: MarkId) {
        let mark_ref = if self.translating_init_code {
            MarkReference::Direct(offset_mark)
        } else {
            MarkReference::Delta(Span::new(self.mark_map.runtime_start, offset_mark))
        };
        self.asm.push_reference(AsmReference { mark_ref, set_size: None, pushed: true });
    }

    fn new(ir: &'ir EthIRProgram) -> Self {
        let memory_layout = StaticMemoryLayout::new(ir);
        let asm = Assembler::with_capacity(ASM_BYTES_CAPACITY, ASM_SECTIONS_CAPACITY);
        let translated_bbs = DenseIndexSet::with_capacity_in_bits(ir.basic_blocks.len());
        let bbs_to_be_translated = Vec::with_capacity(8);
        let mark_map = MarkMap::new(ir);
        Self {
            ir,
            memory_layout,
            asm,
            bbs_to_be_translated,
            mark_map,
            translated_bbs,
            translating_init_code: true,
            source_map_marks: Vec::with_capacity(256),
            global_op_index: 0,
        }
    }

    fn get_bb_mark(&self, bb_id: BasicBlockId) -> MarkId {
        if self.translating_init_code {
            self.mark_map.get_init_bb_mark(bb_id)
        } else {
            self.mark_map.get_run_bb_mark(bb_id)
        }
    }

    fn emit_undefined_behavior_error(&mut self) {
        self.asm.push_minimal_u32(0xbadbad);
        self.asm.push_op_byte(op::PUSH0);
        self.asm.push_op_byte(op::MSTORE);
        self.asm.push_minimal_u32(3);
        self.asm.push_minimal_u32(32 - 3);
        self.asm.push_op_byte(op::REVERT);
    }

    fn translate_basic_blocks_from_entry_point(&mut self, entry_point: FunctionId) {
        let entry_basic_block = self.ir.function(entry_point).entry().id();
        self.bbs_to_be_translated.push((entry_point, entry_basic_block));

        while let Some((func, bb_id)) = self.bbs_to_be_translated.pop() {
            if !self.translated_bbs.add(bb_id) {
                continue;
            }

            self.asm.push_mark(self.get_bb_mark(bb_id));
            self.asm.push_op_byte(op::JUMPDEST);

            let block = self.ir.block(bb_id);
            self.memory_layout.emit_transfer_basic_block_outputs(&mut self.asm, block.inputs());
            for op_view in block.operations() {
                self.push_source_map_mark();
                operations::translate_operation(self, op_view.op());
            }
            self.memory_layout.emit_copy_for_basic_block_inputs(&mut self.asm, block.outputs());

            self.bbs_to_be_translated.extend(block.successors().map(|bb| (func, bb)));

            match block.control() {
                ControlView::LastOpTerminates => {}
                ControlView::InternalReturn => {
                    self.push_source_map_mark();
                    let return_dest_loc = self.memory_layout.get_return_dest_store(func);
                    self.asm.push_minimal_u32(return_dest_loc);
                    self.asm.push_op_byte(op::MLOAD);
                    self.asm.push_op_byte(op::JUMP);
                }
                ControlView::ContinuesTo(to) => {
                    self.push_source_map_mark();
                    self.emit_code_offset_push(self.get_bb_mark(to));
                    self.asm.push_op_byte(op::JUMP);
                }
                ControlView::Branches { condition, non_zero_target, zero_target } => {
                    self.push_source_map_mark();
                    self.emit_local_load(condition);
                    self.emit_code_offset_push(self.get_bb_mark(non_zero_target));
                    self.asm.push_op_byte(op::JUMPI);
                    self.emit_code_offset_push(self.get_bb_mark(zero_target));
                    self.asm.push_op_byte(op::JUMP);
                }
                ControlView::Switch(switch) => {
                    self.push_source_map_mark();
                    self.emit_local_load(switch.condition());
                    self.asm.push_minimal_u32(self.memory_layout.switch_store);
                    self.asm.push_op_byte(op::MSTORE);

                    for (value, bb) in switch.cases() {
                        self.asm.push_minimal_u32(self.memory_layout.switch_store);
                        self.asm.push_op_byte(op::MLOAD);
                        self.asm.push_minimal_u256(value);
                        self.asm.push_op_byte(op::EQ);
                        self.emit_code_offset_push(self.get_bb_mark(bb));
                        self.asm.push_op_byte(op::JUMPI);
                    }

                    if let Some(fallback) = switch.fallback() {
                        self.emit_code_offset_push(self.get_bb_mark(fallback));
                        self.asm.push_op_byte(op::JUMP);
                    } else {
                        self.emit_undefined_behavior_error();
                    };
                }
            }
        }
    }
}

pub fn ir_to_bytecode(ir: &EthIRProgram, result: &mut Vec<u8>) -> Result<(), BytecodeError> {
    ir_to_bytecode_with_source_map(ir, result, None, None)
}

pub fn ir_to_bytecode_with_source_map(
    ir: &EthIRProgram,
    result: &mut Vec<u8>,
    source_map: Option<&mut Vec<SourceMapEntry>>,
    runtime_start_pc: Option<&mut u32>,
) -> Result<(), BytecodeError> {
    let mut translator = Translator::new(ir);

    translator.translating_init_code = true;
    translator.memory_layout.emit_init_free_pointer(&mut translator.asm);
    translator.translate_basic_blocks_from_entry_point(ir.init_entry);

    translator.translating_init_code = false;
    translator.asm.push_mark(translator.mark_map.runtime_start);
    if let Some(main_entry) = ir.main_entry {
        translator.translate_basic_blocks_from_entry_point(main_entry);
    }

    for (data_id, bytes) in ir.data_segments.enumerate_idx() {
        let mark = translator.mark_map.get_data_mark(data_id);
        translator.asm.push_mark(mark);
        translator.asm.push_data(bytes);
    }

    translator.asm.push_mark(translator.mark_map.initcode_end);

    let mark_to_offset = translator
        .asm
        .assemble(result, Some(translator.mark_map.next_mark_id.get() as usize))
        .map_err(BytecodeError::Assemble)?;

    if let Some(runtime_start_pc) = runtime_start_pc {
        *runtime_start_pc = mark_to_offset[translator.mark_map.runtime_start];
    }

    if let Some(source_map) = source_map {
        for &(mark, op_idx) in &translator.source_map_marks {
            source_map.push(SourceMapEntry { op_index: op_idx, pc: mark_to_offset[mark] });
        }
    }
    Ok(())
}
