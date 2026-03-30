use plank_core::{DenseIndexSet, Idx, IncIterable, IndexVec};
use sir_assembler::{AsmReference, Assembler, MarkId, MarkReference, op};
use sir_data::{BasicBlockId, DataId, EthIRProgram, FunctionId, LocalId, Span};
use sir_passes::{
    AnalysesStore, ControlFlowGraphInOutBundling, InOutGroupId, LocalLiveness, Predecessors,
};

use crate::{
    stack_scheduler::stack_machine::StackMachine, static_memory_layout::StaticMemoryLayout,
};

mod operations;
mod stack_scheduler;
mod static_memory_layout;

const ASM_BYTES_CAPACITY: usize = 20_000;
const ASM_SECTIONS_CAPACITY: usize = 512;
const DEFAULT_SPILL_THRESHOLD: u8 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TranslationPhase {
    Init,
    Runtime,
}

struct TranslationContext<'a> {
    pub ir: &'a EthIRProgram,
    pub mark_map: &'a mut MarkMap,
    pub memory_layout: &'a StaticMemoryLayout,
    pub predecessors: &'a Predecessors,
    pub bundling: &'a ControlFlowGraphInOutBundling,
    pub group_layouts: &'a IndexVec<InOutGroupId, Vec<LocalId>>,
    pub bbs_to_be_translated: &'a mut Vec<(FunctionId, BasicBlockId)>,
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

fn compute_group_layouts(
    ir: &EthIRProgram,
    bundling: &ControlFlowGraphInOutBundling,
    liveness: &LocalLiveness,
) -> IndexVec<InOutGroupId, Vec<LocalId>> {
    let mut layouts: IndexVec<InOutGroupId, Vec<LocalId>> =
        IndexVec::from_vec(vec![Vec::new(); bundling.next_group_id().idx()]);

    for block in ir.blocks() {
        let Some(group_id) = bundling.get_out_group(block.id()) else {
            continue;
        };
        if !layouts[group_id].is_empty() {
            continue;
        }
        let live_at_exit = liveness.live_at_exit(block.id());
        let mut layout: Vec<LocalId> = live_at_exit.iter().copied().collect();
        layout.sort();
        layouts[group_id] = layout;
    }

    layouts
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
    fn new(ir: &'ir EthIRProgram, spill_threshold: u8) -> Self {
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
            stack_machine: StackMachine::new(spill_threshold),
        }
    }

    fn translate_basic_blocks_from_entry_point(
        &mut self,
        entry_point: FunctionId,
        predecessors: &Predecessors,
        bundling: &ControlFlowGraphInOutBundling,
        group_layouts: &IndexVec<InOutGroupId, Vec<LocalId>>,
    ) {
        let entry_basic_block = self.ir.function(entry_point).entry().id();
        self.bbs_to_be_translated.push((entry_point, entry_basic_block));

        while let Some((func, bb_id)) = self.bbs_to_be_translated.pop() {
            if !self.translated_bbs.add(bb_id) {
                continue;
            }

            self.asm.push_mark(self.mark_map.get_bb_mark(bb_id));
            self.asm.push_op_byte(op::JUMPDEST);

            let block = self.ir.block(bb_id);
            self.bbs_to_be_translated.extend(block.successors().map(|bb| (func, bb)));

            let mut ctx = TranslationContext {
                ir: self.ir,
                mark_map: &mut self.mark_map,
                memory_layout: &self.memory_layout,
                predecessors,
                bundling,
                group_layouts,
                bbs_to_be_translated: &mut self.bbs_to_be_translated,
            };
            self.stack_machine.dispatch_block(func, bb_id, &mut self.asm, &mut ctx);
        }
    }
}

pub fn ir_to_bytecode(ir: &EthIRProgram, store: &AnalysesStore, result: &mut Vec<u8>) {
    let local_liveness = store.local_liveness(ir);
    let predecessors = store.predecessors(ir);
    let bundling = store.cfg_in_out_bundling(ir);
    let group_layouts = compute_group_layouts(ir, &bundling, &local_liveness);
    let mut translator = Translator::new(ir, DEFAULT_SPILL_THRESHOLD);

    translator.memory_layout.emit_init_free_pointer(&mut translator.asm);
    translator.translate_basic_blocks_from_entry_point(
        ir.init_entry,
        &predecessors,
        &bundling,
        &group_layouts,
    );

    // Ignore translated basic blocks because we want separate PCs for functions and basic
    // blocks in run.
    translator.translated_bbs.clear();
    translator.mark_map.set_phase(TranslationPhase::Runtime);
    translator.asm.push_mark(translator.mark_map.runtime_start);
    if let Some(main_entry) = ir.main_entry {
        translator.translate_basic_blocks_from_entry_point(
            main_entry,
            &predecessors,
            &bundling,
            &group_layouts,
        );
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
