use plank_core::Idx;
use sir_data::{BasicBlockId, EthIRProgram};
use sir_stack_scheduling::{ScheduledOps, stack::StackOps};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

pub struct CsvCollector {
    output_path: PathBuf,
    writer: csv::Writer<File>,
}

struct BlockStats {
    block_id: BasicBlockId,
    operation_count: usize,
    operation_input_count: usize,
    block_input_count: usize,
    scheduled_stack_op_count: usize,
    assumed_gas: u64,
    assumed_code_bytes: u64,
}

impl CsvCollector {
    pub fn create(output_path: PathBuf) -> Self {
        if let Some(parent) = output_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("failed to create '{}': {error}", parent.display()));
        }

        let mut writer = csv::Writer::from_path(&output_path).unwrap_or_else(|error| {
            panic!("failed to create '{}': {error}", output_path.display())
        });
        writer
            .write_record([
                "file",
                "block_id",
                "operation_count",
                "operation_input_count",
                "block_input_count",
                "total_input_count",
                "scheduled_stack_op_count",
                "assumed_gas",
                "assumed_code_bytes",
            ])
            .unwrap();
        Self { output_path, writer }
    }

    pub fn collect(&mut self, file: &Path, program: &EthIRProgram, scheduled: &ScheduledOps) {
        for (block_id, stack_ops) in scheduled.enumerate_idx() {
            let stats = BlockStats::new(program, block_id, stack_ops);
            self.writer
                .write_record([
                    file.display().to_string(),
                    stats.block_id.get().to_string(),
                    stats.operation_count.to_string(),
                    stats.operation_input_count.to_string(),
                    stats.block_input_count.to_string(),
                    (stats.operation_input_count + stats.block_input_count).to_string(),
                    stats.scheduled_stack_op_count.to_string(),
                    stats.assumed_gas.to_string(),
                    stats.assumed_code_bytes.to_string(),
                ])
                .unwrap();
        }
    }

    pub fn finish(mut self) {
        self.writer.flush().unwrap();
        eprintln!("wrote {}", self.output_path.display());
    }
}

impl BlockStats {
    fn new(program: &EthIRProgram, block_id: BasicBlockId, stack_ops: &[StackOps]) -> Self {
        let block = program.block(block_id);
        let operation_count = block.operations().count();
        let operation_input_count =
            block.operations().map(|operation| operation.inputs().len()).sum();
        let (assumed_gas, assumed_code_bytes) = assumed_schedule_cost(stack_ops);
        Self {
            block_id,
            operation_count,
            operation_input_count,
            block_input_count: block.inputs().len(),
            scheduled_stack_op_count: stack_ops.len(),
            assumed_gas,
            assumed_code_bytes,
        }
    }
}

fn assumed_schedule_cost(stack_ops: &[StackOps]) -> (u64, u64) {
    stack_ops.iter().fold((0, 0), |(gas, bytes), operation| {
        let (operation_gas, operation_bytes) = match operation {
            StackOps::Swap(_) | StackOps::Dup(_) | StackOps::Pop => (3, 1),
            StackOps::Exchange(_, _) => (9, 3),
            StackOps::Store(_) => (9, 4),
            StackOps::Load(_) => (6, 4),
            StackOps::Flipped(_) | StackOps::Op(_) | StackOps::CallRetPush(_) => (0, 0),
        };
        (gas + operation_gas, bytes + operation_bytes)
    })
}
