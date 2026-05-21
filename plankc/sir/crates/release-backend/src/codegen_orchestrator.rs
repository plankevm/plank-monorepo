use crate::{
    code_to_asm::{CodeToAsmEmitter, CodegenState},
    mark_map::{IndexableMarkSpan, MarkMap},
};
use hashbrown::HashSet;
use plank_core::{DenseIndexSet, Span};
use sir_assembler::{Assembler, MarkId, MarkReference};
use sir_data::{BasicBlockId, DataId, EthIRProgram, FunctionId, Operation};
use sir_stack_scheduling::ScheduledOps;
use sir_static_memory_allocator as static_mem;

const BB_WORKLIST_START_CAPACITY: usize = 128;
const INIT_ONLY_DATAS_START_CAPACITY: usize = 16;

pub(crate) struct EmitInitcode {
    memory: static_mem::Layout,
    init_only_data: HashSet<DataId>,
    bb_marks: IndexableMarkSpan<BasicBlockId>,
    runtime_datas: DenseIndexSet<DataId>,
}

pub(crate) struct EmitRuncode {
    memory: static_mem::Layout,
    bb_marks: IndexableMarkSpan<BasicBlockId>,
}

pub(crate) struct InitcodeEmitted<'a> {
    emitter: CodeToAsmEmitter<'a>,
    runtime_datas: DenseIndexSet<DataId>,
}

impl CodegenState for EmitInitcode {
    const ALLOW_INITCODE_INTROSPECTION: bool = true;

    fn layout(&self) -> &static_mem::Layout {
        &self.memory
    }

    fn bb_marks(&self) -> IndexableMarkSpan<BasicBlockId> {
        self.bb_marks
    }

    fn mark_to_ref(&self, _marks: &MarkMap, mark: MarkId) -> MarkReference {
        MarkReference::Direct(mark)
    }

    fn data_to_ref(&mut self, marks: &MarkMap, data: DataId) -> MarkReference {
        if !self.runtime_datas.contains(data) {
            self.init_only_data.insert(data);
        }

        let mark = marks.datas.get(data);
        MarkReference::Direct(mark)
    }
}

impl CodegenState for EmitRuncode {
    const ALLOW_INITCODE_INTROSPECTION: bool = false;

    fn layout(&self) -> &static_mem::Layout {
        &self.memory
    }

    fn bb_marks(&self) -> IndexableMarkSpan<BasicBlockId> {
        self.bb_marks
    }

    fn mark_to_ref(&self, marks: &MarkMap, mark: MarkId) -> MarkReference {
        MarkReference::Delta(Span::new(marks.runcode_start, mark))
    }

    fn data_to_ref(&mut self, marks: &MarkMap, data: DataId) -> MarkReference {
        let mark = marks.datas.get(data);
        MarkReference::Delta(Span::new(marks.runcode_start, mark))
    }
}

fn collect_runtime_datas(
    ir: &EthIRProgram,
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

impl<'a> InitcodeEmitted<'a> {
    pub fn emit_init(
        ir: &'a EthIRProgram,
        ops: &'a ScheduledOps,
        init_memory_layout: static_mem::Layout,
    ) -> Self {
        let mut visited_bbs = DenseIndexSet::with_capacity_in_bits(ir.basic_blocks.len());
        let mut basic_blocks_worklist = Vec::with_capacity(BB_WORKLIST_START_CAPACITY);
        let runtime_datas = match ir.main_entry {
            Some(runtime_entrypoint) => {
                let mut runtime_datas =
                    DenseIndexSet::with_capacity_in_bits(ir.data_segments.len());
                collect_runtime_datas(
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

        let mut emitter = CodeToAsmEmitter::new(ir, ops, visited_bbs, basic_blocks_worklist);
        let mut state = EmitInitcode {
            memory: init_memory_layout,
            init_only_data: HashSet::with_capacity(INIT_ONLY_DATAS_START_CAPACITY),
            bb_marks: emitter.alloc_bb_marks(),
            runtime_datas,
        };

        emitter.emit_from_entrypoint(&mut state, ir.init_entry);

        let init_only_datas = {
            let mut init_only_datas_undeterministic: Vec<_> =
                state.init_only_data.iter().copied().collect();
            // Restore determinism by sorting by ID.
            init_only_datas_undeterministic.sort();
            init_only_datas_undeterministic
        };
        for data in init_only_datas {
            emitter.asm.push_mark(emitter.mark_map.datas.get(data));
            emitter.asm.push_data(&ir.data_segments[data]);
        }

        InitcodeEmitted { emitter, runtime_datas: state.runtime_datas }
    }

    pub fn finish_with_runcode(
        self,
        runtime_entrypoint: FunctionId,
        run_memory_layout: static_mem::Layout,
    ) -> (Assembler, MarkMap) {
        let InitcodeEmitted { mut emitter, runtime_datas } = self;

        let mut state =
            EmitRuncode { memory: run_memory_layout, bb_marks: emitter.alloc_bb_marks() };

        emitter.asm.push_mark(emitter.mark_map.runcode_start);
        emitter.emit_from_entrypoint(&mut state, runtime_entrypoint);
        for data in runtime_datas.iter() {
            emitter.asm.push_mark(emitter.mark_map.datas.get(data));
            emitter.asm.push_data(&emitter.ir.data_segments[data]);
        }
        emitter.asm.push_mark(emitter.mark_map.initcode_end);

        (emitter.asm, emitter.mark_map)
    }

    pub fn finish_init_only(self) -> (Assembler, MarkMap) {
        let InitcodeEmitted { mut emitter, runtime_datas: _ } = self;
        emitter.asm.push_mark(emitter.mark_map.runcode_start);
        emitter.asm.push_mark(emitter.mark_map.initcode_end);
        (emitter.asm, emitter.mark_map)
    }
}
