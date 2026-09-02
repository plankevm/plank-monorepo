use clap::Parser;
use hashbrown as _;
use plank_core::{Idx, IncIterable, IndexVec, Span};
use plank_test_utils as _;
use pretty_assertions as _;
use proptest as _;
use rayon as _;
use sir_data::{
    Control, EthIRProgram, LargeConstId, LocalId, Operation, OperationIdx,
    operation::{SetLargeConstData, SetSmallConstData},
};
use sir_parser::{EmitConfig, parse_or_panic};
use sir_passes::{AnalysesStore, Legalizer, run_pass, transforms::CriticalEdgeSplitting};
use sir_stack_scheduling::{ShuffleConfig, schedule, stack::StackOps};
use smallvec as _;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(about = "Collect per-block stack scheduling statistics for a SIR corpus")]
struct Args {
    /// A .sir file or directory recursively containing .sir files.
    input: PathBuf,

    /// Destination CSV file.
    output: PathBuf,

    /// Print every program after benchmark-specific constant canonicalization.
    #[arg(long)]
    print_canonicalized: bool,
}

#[derive(Clone, Copy)]
enum Constant {
    Small(u32),
    Large(LargeConstId),
}

impl Constant {
    fn operation(self, output: LocalId) -> Operation {
        match self {
            Self::Small(value) => {
                Operation::SetSmallConst(SetSmallConstData { sets: output, value })
            }
            Self::Large(value) => {
                Operation::SetLargeConst(SetLargeConstData { sets: output, value })
            }
        }
    }
}

fn main() {
    let args = Args::parse();
    let files = discover_sir_files(&args.input);
    assert!(!files.is_empty(), "no SIR files found under {}", args.input.display());

    if let Some(parent) = args.output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("failed to create '{}': {error}", parent.display()));
    }

    let mut output = csv::Writer::from_path(&args.output)
        .unwrap_or_else(|error| panic!("failed to create '{}': {error}", args.output.display()));
    output
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

    for (file_index, file) in files.iter().enumerate() {
        eprintln!("[{}/{}] {}", file_index + 1, files.len(), file.display());
        let source = fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("failed to read '{}': {error}", file.display()));
        let mut program = parse_or_panic(&source, EmitConfig::default());
        inline_constants_at_each_use(&mut program);
        let analyses = AnalysesStore::default();
        run_pass(&mut CriticalEdgeSplitting, &mut program, &analyses);
        Legalizer::default().run(&program, &analyses).unwrap_or_else(|error| {
            panic!("canonicalized SIR for '{}' is illegal: {error}", file.display())
        });

        if args.print_canonicalized {
            println!("=== {} ===\n{program}", file.display());
        }

        write_program_stats(&mut output, display_path(&args.input, file), &program);
    }

    output.flush().unwrap();
    eprintln!("wrote {}", args.output.display());
}

fn discover_sir_files(input: &Path) -> Vec<PathBuf> {
    assert!(input.exists(), "input '{}' does not exist", input.display());
    let mut files = Vec::new();
    discover_sir_files_into(input, &mut files);
    files.sort();
    files
}

fn discover_sir_files_into(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if path.extension().and_then(|extension| extension.to_str()) == Some("sir")
            && !name.starts_with("._")
        {
            files.push(path.to_owned());
        }
        return;
    }

    if path.file_name().and_then(|name| name.to_str()) == Some("__MACOSX") {
        return;
    }

    for entry in fs::read_dir(path)
        .unwrap_or_else(|error| panic!("failed to read directory '{}': {error}", path.display()))
    {
        let entry = entry.unwrap();
        discover_sir_files_into(&entry.path(), files);
    }
}

fn display_path<'a>(input: &Path, file: &'a Path) -> &'a Path {
    if input.is_dir() { file.strip_prefix(input).unwrap() } else { file }
}

fn inline_constants_at_each_use(program: &mut EthIRProgram) {
    let mut constants = IndexVec::<LocalId, Option<Constant>>::new();
    constants.resize(program.next_free_local_id.idx(), None);
    for operation in program.operations.iter() {
        match operation {
            Operation::SetSmallConst(data) => {
                constants[data.sets] = Some(Constant::Small(data.value))
            }
            Operation::SetLargeConst(data) => {
                constants[data.sets] = Some(Constant::Large(data.value))
            }
            _ => {}
        }
    }

    let old_operations = std::mem::take(&mut program.operations);
    let mut operations = IndexVec::<OperationIdx, Operation>::with_capacity(old_operations.len());
    let block_ids = program.basic_blocks.iter_idx().collect::<Vec<_>>();

    for block_id in block_ids {
        let old_span = program.basic_blocks[block_id].operations;
        let new_start = operations.next_idx();

        for operation_id in old_span.iter() {
            let mut operation = old_operations[operation_id];
            if matches!(operation, Operation::SetSmallConst(_) | Operation::SetLargeConst(_)) {
                continue;
            }

            let (locals, next_local) = (&mut program.locals, &mut program.next_free_local_id);
            for input in operation.inputs_mut(locals) {
                let Some(constant) = constants.get(*input).copied().flatten() else { continue };
                let fresh = next_local.get_and_inc();
                operations.push(constant.operation(fresh));
                *input = fresh;
            }
            operations.push(operation);
        }

        let outputs = program.basic_blocks[block_id].outputs;
        for output_index in outputs.iter() {
            let original = program.locals[output_index];
            let Some(constant) = constants.get(original).copied().flatten() else { continue };
            let fresh = program.next_free_local_id.get_and_inc();
            operations.push(constant.operation(fresh));
            program.locals[output_index] = fresh;
        }

        let control_input = match program.basic_blocks[block_id].control {
            Control::Branches(branch) => Some(branch.condition),
            Control::Switch(switch) => Some(switch.condition),
            Control::LastOpTerminates | Control::InternalReturn | Control::ContinuesTo(_) => None,
        };
        if let Some(original) = control_input
            && let Some(constant) = constants.get(original).copied().flatten()
        {
            let fresh = program.next_free_local_id.get_and_inc();
            operations.push(constant.operation(fresh));
            match &mut program.basic_blocks[block_id].control {
                Control::Branches(branch) => branch.condition = fresh,
                Control::Switch(switch) => switch.condition = fresh,
                Control::LastOpTerminates | Control::InternalReturn | Control::ContinuesTo(_) => {
                    unreachable!()
                }
            }
        }

        program.basic_blocks[block_id].operations = Span::new(new_start, operations.next_idx());
    }

    program.operations = operations;
}

fn write_program_stats(
    output: &mut csv::Writer<std::fs::File>,
    file: &Path,
    program: &EthIRProgram,
) {
    let analyses = AnalysesStore::default();
    let (scheduled, _layouts, _next_alloc_id) =
        schedule(program, &analyses, ShuffleConfig::PRE_AMSTERDAM);

    for (block_id, stack_ops) in scheduled.enumerate_idx() {
        let block = program.block(block_id);
        let operation_count = block.operations().count();
        let operation_input_count =
            block.operations().map(|operation| operation.inputs().len()).sum::<usize>();
        let block_input_count = block.inputs().len();
        let (assumed_gas, assumed_code_bytes) = assumed_schedule_cost(stack_ops);

        output
            .write_record([
                file.display().to_string(),
                block_id.get().to_string(),
                operation_count.to_string(),
                operation_input_count.to_string(),
                block_input_count.to_string(),
                (operation_input_count + block_input_count).to_string(),
                stack_ops.len().to_string(),
                assumed_gas.to_string(),
                assumed_code_bytes.to_string(),
            ])
            .unwrap();
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
