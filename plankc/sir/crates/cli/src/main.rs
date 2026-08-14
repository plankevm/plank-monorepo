use clap::Parser;
use sir_parser::{EmitConfig, parse_or_panic};
use sir_passes::{
    OptimizationLevel, PASSES_HELP, PassManager, parse_passes, run_pass,
    transforms::CriticalEdgeSplitting,
};
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};

#[derive(Parser)]
#[command(name = "sir")]
#[command(about = "Sensei IR to EVM bytecode compiler", long_about = None)]
#[command(version)]
struct Cli {
    /// Input file (use '-' or omit for stdin)
    input: Option<PathBuf>,

    /// Compile only init function (no main)
    #[arg(long)]
    init_only: bool,

    /// Override init function name
    #[arg(long, default_value = "init")]
    init_name: String,

    /// Override main function name
    #[arg(long, default_value = "main")]
    main_name: String,

    #[arg(
        short = 'O',
        long,
        conflicts_with = "passes",
        help = "Optimization level: O0 or O2. Default is O0"
    )]
    optimize: Option<OptimizationLevel>,

    #[arg(long, help = PASSES_HELP, value_parser = parse_passes, conflicts_with = "optimize")]
    passes: Option<String>,
}

fn read_input(input: Option<PathBuf>) -> String {
    let use_stdin = match &input {
        None => true,
        Some(path) => path.to_str() == Some("-"),
    };

    if use_stdin {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).expect("stdin read to succeed");
        buffer
    } else {
        let path = input.unwrap();
        fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read file '{}': {}", path.display(), e))
    }
}

fn main() {
    let cli = Cli::parse();
    let optimization_level = cli.optimize.unwrap_or_default();

    // Read input source
    let source = read_input(cli.input);

    // Build emit configuration
    let config = if cli.init_only {
        EmitConfig::init_only_with_name(&cli.init_name)
    } else {
        EmitConfig::new(&cli.init_name, &cli.main_name)
    };

    // Parse IR to EthIRProgram
    let mut program = parse_or_panic(&source, config);

    let mut pass_manager = PassManager::new(&mut program);
    if let Some(passes) = &cli.passes {
        pass_manager.run_optimizations(passes);
    } else if let Some(passes) = optimization_level.passes() {
        pass_manager.run_optimizations(passes);
    }
    let analyses = pass_manager.into_store();

    let mut bytecode = Vec::with_capacity(0x6000);
    match optimization_level {
        OptimizationLevel::O0 => sir_debug_backend::ir_to_bytecode(&program, &mut bytecode),
        OptimizationLevel::O2 => {
            run_pass(&mut CriticalEdgeSplitting, &mut program, &analyses);
            sir_release_backend::ir_to_bytecode(&program, &analyses, &mut bytecode);
        }
    }

    // Format and print output
    println!("{:#}", alloy_primitives::hex::display(bytecode));
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn passes_and_optimize_are_mutually_exclusive() {
        let error = Cli::try_parse_from(["sir", "-O2", "--passes", "i"]).err().unwrap();
        assert_eq!(error.kind(), ErrorKind::ArgumentConflict);
    }
}
