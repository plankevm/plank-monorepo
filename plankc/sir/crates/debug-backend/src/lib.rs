use plank_core::{DenseIndexSet, Idx, IncIterable};
use sir_assembler::{AsmReference, Assembler, MarkId, MarkReference, op};
use sir_data::{BasicBlockId, DataId, EthIRProgram, FunctionId, Span};
use sir_passes::{AnalysesStore, LocalLiveness};

use crate::{
    stack_scheduler::stack_machine::StackMachine, static_memory_layout::StaticMemoryLayout,
};

mod operations;
mod stack_scheduler;
mod static_memory_layout;

const ASM_BYTES_CAPACITY: usize = 20_000;
const ASM_SECTIONS_CAPACITY: usize = 512;
const DEFAULT_SPILL_THRESHOLD: u8 = 16;
const DEFAULT_CLEANUP_THRESHOLD: u16 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranslationPhase {
    Init,
    Runtime,
}

struct TranslationContext<'a> {
    pub ir: &'a EthIRProgram,
    pub mark_map: &'a MarkMap,
    pub memory_layout: &'a StaticMemoryLayout,
    pub local_liveness: &'a LocalLiveness,
}

struct MarkMap {
    init_basic_block_marks_start: MarkId,
    run_basic_block_marks_start: MarkId,
    data_marks_start: MarkId,
    runtime_start: MarkId,
    initcode_end: MarkId,
    next_mark_id: MarkId,
    phase: TranslationPhase,
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
            phase: TranslationPhase::Init,
        }
    }

    pub fn set_phase(&mut self, phase: TranslationPhase) {
        self.phase = phase;
    }

    pub fn phase(&self) -> TranslationPhase {
        self.phase
    }

    pub fn allocate_mark(&mut self) -> MarkId {
        self.next_mark_id.get_and_inc()
    }

    pub fn get_bb_mark(&self, bb_id: BasicBlockId) -> MarkId {
        match self.phase {
            TranslationPhase::Init => self.init_basic_block_marks_start + bb_id.get(),
            TranslationPhase::Runtime => self.run_basic_block_marks_start + bb_id.get(),
        }
    }

    pub fn get_data_mark(&self, data_id: DataId) -> MarkId {
        self.data_marks_start + data_id.get()
    }

    pub fn emit_code_offset_push(&self, asm: &mut Assembler, offset_mark: MarkId) {
        let mark_ref = match self.phase {
            TranslationPhase::Init => MarkReference::Direct(offset_mark),
            TranslationPhase::Runtime => {
                MarkReference::Delta(Span::new(self.runtime_start, offset_mark))
            }
        };
        asm.push_reference(AsmReference { mark_ref, set_size: None, pushed: true });
    }
}

struct Translator<'ir> {
    pub ir: &'ir EthIRProgram,
    pub memory_layout: StaticMemoryLayout,
    pub mark_map: MarkMap,
    pub translated_bbs: DenseIndexSet<BasicBlockId>,
    pub bbs_to_be_translated: Vec<(FunctionId, BasicBlockId)>,
    pub asm: Assembler,
    pub stack_machine: StackMachine,
}

impl<'ir> Translator<'ir> {
    fn new(ir: &'ir EthIRProgram, spill_threshold: u8, cleanup_threshold: u16) -> Self {
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
            stack_machine: StackMachine::new(spill_threshold, cleanup_threshold),
        }
    }

    fn get_bb_mark(&self, bb_id: BasicBlockId) -> MarkId {
        self.mark_map.get_bb_mark(bb_id)
    }

    fn translate_basic_blocks_from_entry_point(
        &mut self,
        entry_point: FunctionId,
        local_liveness: &LocalLiveness,
    ) {
        let entry_basic_block = self.ir.function(entry_point).entry().id();
        self.bbs_to_be_translated.push((entry_point, entry_basic_block));

        let ctx = TranslationContext {
            ir: self.ir,
            mark_map: &self.mark_map,
            memory_layout: &self.memory_layout,
            local_liveness,
        };

        while let Some((func, bb_id)) = self.bbs_to_be_translated.pop() {
            if !self.translated_bbs.add(bb_id) {
                continue;
            }

            self.asm.push_mark(self.get_bb_mark(bb_id));
            self.asm.push_op_byte(op::JUMPDEST);

            let block = self.ir.block(bb_id);
            self.bbs_to_be_translated.extend(block.successors().map(|bb| (func, bb)));

            self.stack_machine.dispatch_block(func, bb_id, &mut self.asm, &ctx);
        }
    }
}

pub fn ir_to_bytecode(ir: &EthIRProgram, store: &AnalysesStore, result: &mut Vec<u8>) {
    let local_liveness = store.local_liveness(ir);
    let mut translator = Translator::new(ir, DEFAULT_SPILL_THRESHOLD, DEFAULT_CLEANUP_THRESHOLD);

    translator.memory_layout.emit_init_free_pointer(&mut translator.asm);
    translator.translate_basic_blocks_from_entry_point(ir.init_entry, &local_liveness);

    // Ignore translated basic blocks because we want separate PCs for functions and basic
    // blocks in run.
    translator.translated_bbs.clear();
    translator.mark_map.set_phase(TranslationPhase::Runtime);
    translator.asm.push_mark(translator.mark_map.runtime_start);
    if let Some(main_entry) = ir.main_entry {
        translator.translate_basic_blocks_from_entry_point(main_entry, &local_liveness);
    }

    for (data_id, bytes) in ir.data_segments.enumerate_idx() {
        let mark = translator.mark_map.get_data_mark(data_id);
        translator.asm.push_mark(mark);
        translator.asm.push_data(bytes);
    }

    translator.asm.push_mark(translator.mark_map.initcode_end);

    let _mark_to_offset = translator
        .asm
        .assemble(result, Some(translator.mark_map.next_mark_id.get() as usize))
        .expect("debug backend produces valid assembly");
}
