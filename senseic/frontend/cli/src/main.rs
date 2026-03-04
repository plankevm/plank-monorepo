use clap::Parser;
use sensei_hir::{BigNumInterner, display::DisplayHir, lower};
use sensei_mir::display::DisplayMir;
use sensei_parser::{
    cst::display::DisplayCST,
    error_report::{ErrorCollector, LineIndex, format_error},
    interner::PlankInterner,
    lexer::Lexed,
    parser::parse,
};
use sir_optimizations::{Optimizer, parse_passes_string};

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
}

fn main() {
    let args = Args::parse();
    let source = std::fs::read_to_string(&args.file_path).expect("Failed to read file");

    let lexed = Lexed::lex(&source);
    let mut collector = ErrorCollector::default();
    let mut interner = PlankInterner::default();
    let cst = parse(&lexed, &mut interner, &mut collector);

    if args.show_cst {
        let display = DisplayCST::new(&cst, &source, &lexed).show_line(args.show_lines);
        println!("{}", display);
    }

    if !collector.errors.is_empty() {
        let line_index = LineIndex::new(&source);
        for error in &collector.errors {
            eprintln!("{}\n", format_error(error, &source, &line_index));
        }

        std::process::exit(1);
    }

    let mut big_nums = BigNumInterner::new();
    let hir = lower(&cst, &mut big_nums);

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
