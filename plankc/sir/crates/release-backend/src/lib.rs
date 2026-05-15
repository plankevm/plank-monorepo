use sir_data::EthIRProgram;
use sir_passes::AnalysesStore;
use sir_stack_scheduling::{self, ScheduleConfig};
use sir_static_memory_allocator::BumpAllocateAll;

mod code_to_asm;
mod mark_map;
mod translator;

pub fn ir_to_bytecode(program: &EthIRProgram, analyses: &AnalysesStore, bytecode: &mut Vec<u8>) {
    let (stack_ops, _layouts) =
        sir_stack_scheduling::schedule(program, analyses, ScheduleConfig::PRE_AMSTERDAM);
    let init_mem_layout = BumpAllocateAll::generate(program, program.init_entry, &stack_ops);
}
