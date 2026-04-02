use clap::Parser;
use sir_debug_backend::SourceMapEntry;
use sir_optimizations::Optimizer;
use sir_parser::{EmitConfig, parse_ir};
use std::{
    fs,
    io::{self, Read, Write},
    path::PathBuf,
};

fn parse_optimization_passes(s: &str) -> Result<String, String> {
    for c in s.chars() {
        if !matches!(c, 's' | 'c' | 'u' | 'd') {
            return Err(format!(
                "invalid optimization pass '{}', valid passes: s (SCCP), c (copy propagation), u (unused elimination), d (defragment)",
                c
            ));
        }
    }
    Ok(s.to_string())
}

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

    /// Optimization passes to run in order. Each character is a pass:
    /// s = SCCP (constant propagation),
    /// c = copy propagation,
    /// u = unused operation elimination,
    /// d = defragment.
    /// Example: -O csud
    #[arg(short = 'O', long = "optimize", value_parser = parse_optimization_passes)]
    optimize: Option<String>,

    /// Write source map (op_index -> bytecode_pc) to this file
    #[arg(long)]
    source_map: Option<PathBuf>,
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

    // Read input source
    let source = read_input(cli.input);

    // Build emit configuration
    let config = if cli.init_only {
        EmitConfig::init_only_with_name(&cli.init_name)
    } else {
        EmitConfig::new(&cli.init_name, &cli.main_name)
    };

    // Parse IR to EthIRProgram
    let mut program = match parse_ir(&source, config) {
        Ok(program) => program,
        Err(err) => {
            eprintln!("{}", err.render_with_source(&source, 2));
            std::process::exit(1);
        }
    };

    if let Some(passes) = cli.optimize {
        let mut optimizer = Optimizer::new(program);
        optimizer.run_passes(&passes);
        program = optimizer.finish();
    }

    let mut bytecode = Vec::with_capacity(0x6000);
    if let Some(ref source_map_path) = cli.source_map {
        let mut source_map = Vec::with_capacity(256);
        let mut runtime_start_pc = 0u32;
        if let Err(err) = sir_debug_backend::ir_to_bytecode_with_source_map(&program, &mut bytecode, Some(&mut source_map), Some(&mut runtime_start_pc)) {
            eprintln!("Failed to generate bytecode: {err}");
            std::process::exit(1);
        }
        write_source_map(source_map_path, &source_map, runtime_start_pc);
    } else if let Err(err) = sir_debug_backend::ir_to_bytecode(&program, &mut bytecode) {
        eprintln!("Failed to generate bytecode: {err}");
        std::process::exit(1);
    }

    // Format and print output
    print!("0x");
    for byte in bytecode {
        print!("{:02x}", byte);
    }
    println!();
}

fn write_source_map(path: &PathBuf, entries: &[SourceMapEntry], runtime_start_pc: u32) {
    let mut out = String::with_capacity(entries.len() * 20);
    out.push_str(&format!("{{\"runtime_start_pc\":{},\"ops\":[", runtime_start_pc));
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"idx\":{},\"pc\":{}}}", entry.op_index, entry.pc));
    }
    out.push_str("]}");

    let mut file = fs::File::create(path)
        .unwrap_or_else(|e| panic!("failed to create source map '{}': {}", path.display(), e));
    file.write_all(out.as_bytes())
        .unwrap_or_else(|e| panic!("failed to write source map '{}': {}", path.display(), e));
}
