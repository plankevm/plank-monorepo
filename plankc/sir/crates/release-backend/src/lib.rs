use crate::codegen_orchestrator::InitcodeEmitted;
use sir_data::EthIRProgram;
use sir_passes::AnalysesStore;
use sir_stack_scheduling::{self, ScheduleConfig};
use sir_static_memory_allocator::BumpAllocateAll;

mod code_to_asm;
mod codegen_orchestrator;
mod mark_map;

pub fn ir_to_bytecode(program: &EthIRProgram, analyses: &AnalysesStore, bytecode: &mut Vec<u8>) {
    let (stack_ops, _layouts) =
        sir_stack_scheduling::schedule(program, analyses, ScheduleConfig::PRE_AMSTERDAM);
    let init_memory_layout = BumpAllocateAll::generate(program, program.init_entry, &stack_ops);

    let in_progress_codegen = InitcodeEmitted::emit_init(program, &stack_ops, init_memory_layout);
    let (asm, marks) = match program.main_entry {
        Some(runtime_entrypoint) => {
            let run_memory_layout =
                BumpAllocateAll::generate(program, runtime_entrypoint, &stack_ops);
            in_progress_codegen.finish_with_runcode(runtime_entrypoint, run_memory_layout)
        }
        None => in_progress_codegen.finish_init_only(),
    };

    asm.assemble(bytecode, Some(marks.next_mark_id.const_get() as usize))
        .expect("generated invalid asm");
}
