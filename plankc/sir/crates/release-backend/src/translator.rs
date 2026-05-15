use crate::code_to_asm::{CodeToAsmEmitter, CodegenState};
use hashbrown::HashSet;
use sir_assembler::{MarkId, MarkReference};
use sir_data::{BasicBlockId, DataId, EthIRProgram, FunctionId, Span};
use sir_stack_scheduling::ScheduledOps;
use sir_static_memory_allocator as static_mem;

const INIT_ONLY_DATAS_CAPACITY: usize = 16;

pub(crate) struct EmitInitcode {
    memory: static_mem::Layout,
    init_only_data: HashSet<DataId>,
    bb_marks: Span<MarkId>,
}

pub(crate) struct EmitRuncode {
    memory: static_mem::Layout,
    entrypoint: FunctionId,
    bb_marks: Span<MarkId>,
}

pub(crate) struct CodegenOrchestrator<'a, State: CodegenState> {
    emitter: CodeToAsmEmitter<'a>,
    state: State,
}

impl CodegenState for EmitInitcode {
    const ALLOW_INITCODE_INTROSPECTION: bool = true;

    fn layout(&self) -> &sir_static_memory_allocator::Layout {
        &self.memory
    }

    fn bb_to_jumpdest_mark(&self, bb: BasicBlockId) -> MarkId {
        let mark = self.bb_marks.start + bb.const_get();
        assert!(mark < self.bb_marks.end, "unexpected basic block id");
        mark
    }

    fn mark_to_ref(&self, _map: &crate::mark_map::MarkMap, mark: MarkId) -> MarkReference {
        MarkReference::Direct(mark)
    }
}

impl CodegenState for EmitRuncode {
    const ALLOW_INITCODE_INTROSPECTION: bool = false;

    fn layout(&self) -> &sir_static_memory_allocator::Layout {
        &self.memory
    }

    fn bb_to_jumpdest_mark(&self, bb: BasicBlockId) -> MarkId {
        let mark = self.bb_marks.start + bb.const_get();
        assert!(mark < self.bb_marks.end, "unexpected basic block id");
        mark
    }

    fn mark_to_ref(&self, map: &crate::mark_map::MarkMap, mark: MarkId) -> MarkReference {
        MarkReference::Delta(Span::new(map.runcode_start, mark))
    }
}

impl<'a> CodegenOrchestrator<'a, EmitInitcode> {
    pub fn begin(
        ir: &'a EthIRProgram,
        ops: &'a ScheduledOps,
        init_memory_layout: static_mem::Layout,
    ) -> Self {
        let mut emitter = CodeToAsmEmitter::new(ir, ops);
        let init_bb_marks = emitter.alloc_bb_marks();
        let mut state = EmitInitcode {
            memory: init_memory_layout,
            init_only_data: HashSet::with_capacity(INIT_ONLY_DATAS_CAPACITY),
            bb_marks: init_bb_marks,
        };

        emitter.emit_from_entrypoint(&mut state, ir.init_entry);

        CodegenOrchestrator { emitter, state }
    }
}
