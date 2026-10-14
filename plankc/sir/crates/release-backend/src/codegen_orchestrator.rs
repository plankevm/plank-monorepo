use crate::{
    code_to_asm::{CodeToAsmEmitter, CodegenState},
    immutables::{Counting, EmittingGets, EmittingSets, ImmutableRefs},
    mark_map::{IndexableMarkSpan, MarkMap},
};
use hashbrown::HashSet;
use plank_core::{DenseIndexSet, Span};
use sir_assembler::{Assembler, MarkId, MarkReference};
use sir_data::{
    BasicBlockId, DataId, EthIRProgram, FunctionId, ImmutableId, Operation, operation::ByteSize,
};
use sir_stack_scheduling::ScheduledOps;
use sir_static_memory_allocator as static_mem;

const BB_WORKLIST_START_CAPACITY: usize = 128;
const INIT_ONLY_DATAS_START_CAPACITY: usize = 16;

pub(crate) struct EmitInitcode {
    memory: static_mem::Layout,
    init_only_data: HashSet<DataId>,
    bb_marks: IndexableMarkSpan<BasicBlockId>,
    runtime_datas: DenseIndexSet<DataId>,
    immutables: ImmutableRefs<EmittingSets>,
}

pub(crate) struct EmitRuncode {
    memory: static_mem::Layout,
    bb_marks: IndexableMarkSpan<BasicBlockId>,
    immutables: ImmutableRefs<EmittingGets>,
}

pub(crate) struct InitcodeEmitted<'a> {
    emitter: CodeToAsmEmitter<'a>,
    runtime_datas: DenseIndexSet<DataId>,
    immutables: ImmutableRefs<EmittingGets>,
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

    fn get_immutable(&mut self, _asm: &mut Assembler, _immutable: ImmutableId, _size: ByteSize) {
        panic!("use of `getimmutable` outside of runtime code")
    }

    fn set_immutable(
        &self,
        asm: &mut Assembler,
        marks: &MarkMap,
        immutable: ImmutableId,
        size: ByteSize,
    ) {
        self.immutables.emit_set(
            asm,
            marks.runcode_start,
            self.memory.scratch_slot,
            immutable,
            size,
        );
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

    fn get_immutable(&mut self, asm: &mut Assembler, immutable: ImmutableId, size: ByteSize) {
        self.immutables.emit_placeholder(asm, immutable, size);
    }

    fn set_immutable(
        &self,
        _asm: &mut Assembler,
        _marks: &MarkMap,
        _immutable: ImmutableId,
        _size: ByteSize,
    ) {
        panic!("use of `setimmutable` outside of initcode")
    }
}

fn collect_runtime_references(
    ir: &EthIRProgram,
    visited_bbs: &mut DenseIndexSet<BasicBlockId>,
    basic_blocks_worklist: &mut Vec<BasicBlockId>,
    runtime_datas: &mut DenseIndexSet<DataId>,
    mut immutable_refs: ImmutableRefs<Counting>,
    next_mark_id: &mut MarkId,
    runtime_entrypoint: FunctionId,
) -> ImmutableRefs<EmittingSets> {
    ir.for_each_reachable_operation(runtime_entrypoint, visited_bbs, basic_blocks_worklist, |op| {
        match op.op() {
            Operation::SetDataOffset(set_data) => {
                runtime_datas.add(set_data.segment_id);
            }
            Operation::GetImmutable(get) => {
                immutable_refs.count_ref(get.immutable);
            }
            _ => {}
        }
    });
    immutable_refs.alloc_marks(next_mark_id)
}

impl<'a> InitcodeEmitted<'a> {
    pub fn emit_init(
        ir: &'a EthIRProgram,
        ops: &'a ScheduledOps,
        generate_init_memory_layout: impl FnOnce(bool) -> static_mem::Layout,
    ) -> Self {
        let mut visited_bbs = DenseIndexSet::with_capacity_in_bits(ir.basic_blocks.len());
        let mut basic_blocks_worklist = Vec::with_capacity(BB_WORKLIST_START_CAPACITY);
        let mut mark_map = MarkMap::new(ir);
        let immutable_refs = ImmutableRefs::new(ir);
        let (runtime_datas, immutables) = match ir.main_entry {
            Some(runtime_entrypoint) => {
                let mut runtime_datas =
                    DenseIndexSet::with_capacity_in_bits(ir.data_segments.len());
                let immutables = collect_runtime_references(
                    ir,
                    &mut visited_bbs,
                    &mut basic_blocks_worklist,
                    &mut runtime_datas,
                    immutable_refs,
                    &mut mark_map.next_mark_id,
                    runtime_entrypoint,
                );
                (runtime_datas, immutables)
            }
            None => (DenseIndexSet::new(), immutable_refs.alloc_marks(&mut mark_map.next_mark_id)),
        };

        let init_memory_layout = generate_init_memory_layout(immutables.needs_scratch(ir));
        let mut emitter =
            CodeToAsmEmitter::new(ir, ops, mark_map, visited_bbs, basic_blocks_worklist);
        let mut state = EmitInitcode {
            memory: init_memory_layout,
            init_only_data: HashSet::with_capacity(INIT_ONLY_DATAS_START_CAPACITY),
            bb_marks: emitter.alloc_bb_marks(),
            runtime_datas,
            immutables,
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

        InitcodeEmitted {
            emitter,
            runtime_datas: state.runtime_datas,
            immutables: state.immutables.into_emit_gets(),
        }
    }

    pub fn finish_with_runcode(
        self,
        runtime_entrypoint: FunctionId,
        run_memory_layout: static_mem::Layout,
    ) -> (Assembler, MarkMap) {
        let InitcodeEmitted { mut emitter, runtime_datas, immutables } = self;

        let mut state = EmitRuncode {
            memory: run_memory_layout,
            bb_marks: emitter.alloc_bb_marks(),
            immutables,
        };

        emitter.asm.push_mark(emitter.mark_map.runcode_start);
        emitter.emit_from_entrypoint(&mut state, runtime_entrypoint);
        state.immutables.assert_all_emitted();
        for data in runtime_datas.iter() {
            emitter.asm.push_mark(emitter.mark_map.datas.get(data));
            emitter.asm.push_data(&emitter.ir.data_segments[data]);
        }
        emitter.asm.push_mark(emitter.mark_map.initcode_end);

        (emitter.asm, emitter.mark_map)
    }

    pub fn finish_init_only(self) -> (Assembler, MarkMap) {
        let InitcodeEmitted { mut emitter, runtime_datas: _, immutables: _ } = self;
        emitter.asm.push_mark(emitter.mark_map.runcode_start);
        emitter.asm.push_mark(emitter.mark_map.initcode_end);
        (emitter.asm, emitter.mark_map)
    }
}
