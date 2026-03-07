use clap::Parser;
use sensei_hir::{BigNumInterner, display::DisplayHir, lower};
use sensei_mir::display::DisplayMir;
use sensei_parser::{
    SourceId,
    cst::display::DisplayCST,
    error_report::{ErrorCollector, LineIndex, format_error},
    interner::PlankInterner,
    lexer::Lexed,
    module::ModuleResolver,
    project::parse_project,
    source_fs::RealFs,
};
use sir_optimizations::{Optimizer, parse_passes_string};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "senseic", about = "Sensei compiler frontend")]
struct Args {
    file_path: String,

    #[arg(short = 'l', long = "show-lines", help = "enables line numbers in the CST output")]
    show_lines: bool,

    #[arg(short = 'c', long = "show-cst", help = "show CST")]
    show_cst: bool,

    #[arg(long = "show-hir", help = "show HIR")]
    show_hir: bool,

    #[arg(short = 'm', long = "show-mir", help = "show MIR")]
    show_mir: bool,

    /// Optimization passes to run in order. Each character is a pass:
    /// s = SCCP (constant propagation),
    /// c = copy propagation,
    /// u = unused operation elimination,
    /// d = defragment.
    /// Example: -O csud
    #[arg(short = 'O', long = "optimize", value_parser = parse_passes_string)]
    optimize: Option<String>,

    #[arg(long = "already-ssa")]
    already_ssa: bool,

    #[arg(long = "module-name")]
    module_name: Option<String>,

    #[arg(long = "module-root", requires = "module_name")]
    module_root: Option<String>,

    #[arg(long = "dep", value_parser = parse_dep)]
    deps: Vec<(String, PathBuf)>,
}

fn parse_dep(s: &str) -> Result<(String, PathBuf), String> {
    let (name, path) =
        s.split_once('=').ok_or_else(|| format!("expected format name=path, got '{s}'"))?;
    Ok((name.to_string(), PathBuf::from(path)))
}

fn main() {
    let args = Args::parse();
    let mut interner = PlankInterner::default();
    let mut module_resolver = ModuleResolver::default();
    if let Some(name) = &args.module_name {
        let name_id = interner.intern(name);
        let root = match &args.module_root {
            Some(root) => PathBuf::from(root),
            None => Path::new(&args.file_path)
                .parent()
                .expect("file path has no parent directory")
                .to_path_buf(),
        };
        module_resolver.register(name_id, root);
    }
    for (name, path) in &args.deps {
        let name_id = interner.intern(name);
        module_resolver.register(name_id, path.clone());
    }

    let mut collector = ErrorCollector::default();
    let project = parse_project(
        Path::new(&args.file_path),
        &module_resolver,
        &mut interner,
        &mut collector,
        &RealFs,
    );

    if args.show_cst {
        let entry = &project.sources[SourceId::ROOT];
        let lexed = Lexed::lex(&entry.content);
        let display =
            DisplayCST::new(&entry.cst, &entry.content, &lexed).show_line(args.show_lines);
        println!("{}", display);
    }

    if !collector.errors.is_empty() {
        for (source_id, error) in &collector.errors {
            let source = &project.sources[*source_id];
            let line_index = LineIndex::new(&source.content);
            eprintln!("{}\n", format_error(error, &source.content, &line_index));
        }
        std::process::exit(1);
    }

    let mut big_nums = BigNumInterner::new();
    let hir = lower(&project, &mut big_nums);

    if args.show_hir {
        if args.show_mir {
            println!("////////////////////////////////////////////////////////////////");
            println!("//                            HIR                             //");
            println!("////////////////////////////////////////////////////////////////");
        }
        print!("{}", DisplayHir::new(&hir, &big_nums, &interner));
        if args.show_mir {
            println!("\n");
            println!("////////////////////////////////////////////////////////////////");
            println!("//                            MIR                             //");
            println!("////////////////////////////////////////////////////////////////");
        }
    }

    let mir = sensei_hir_eval::evaluate(&hir);

    if args.show_mir {
        print!("{}", DisplayMir::new(&mir, &big_nums));
    }

    let mut program = sensei_mir_lower::lower(&mir, &big_nums);
    if args.already_ssa {
        sir_analyses::legalize(&program).expect("illegal IR pre-ssa");
    }
    sir_transforms::ssa_transform(&mut program);
    sir_analyses::legalize(&program).expect("illegal IR post ssa transform");

    if let Some(passes) = args.optimize {
        let mut optimizer = Optimizer::new(program);
        optimizer.run_passes(&passes);
        program = optimizer.finish();
    }

    let mut bytecode = Vec::with_capacity(0x6000);
    sir_debug_backend::ir_to_bytecode(&program, &mut bytecode);

    // Format and print output
    print!("0x");
    for byte in bytecode {
        print!("{:02x}", byte);
    }
    println!();
}
