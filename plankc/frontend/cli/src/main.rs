use clap::Parser;
use plank_hir::{display::DisplayHir, lower};
use plank_mir::display::DisplayMir;
use plank_parser::cst::display::DisplayCST;
use plank_session::{Session, SourceId};
use plank_source::{ModuleResolver, parse_project, source_fs::RealFs};
use plank_values::BigNumInterner;
use sir_passes::{OPTIMIZE_HELP, PassManager, parse_optimizations_string};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "plank", about = "Plank compiler frontend")]
struct Args {
    file_path: String,

    #[arg(short = 'c', long = "show-cst", help = "show CST")]
    show_cst: bool,

    #[arg(long = "show-hir", help = "show HIR")]
    show_hir: bool,

    #[arg(short = 'm', long = "show-mir", help = "show MIR")]
    show_mir: bool,

    #[arg(short = 'O', long = "optimize", help = OPTIMIZE_HELP, value_parser = parse_optimizations_string)]
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
    let mut session = Session::new();
    let mut module_resolver = ModuleResolver::default();
    if let Some(name) = &args.module_name {
        let name_id = session.intern(name);
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
        let name_id = session.intern(name);
        module_resolver.register(name_id, path.clone());
    }

    let project =
        parse_project(Path::new(&args.file_path), &module_resolver, &mut session, &RealFs);

    if args.show_cst {
        let parsed = &project.parsed_sources[SourceId::ROOT];
        let source = session.get_source(SourceId::ROOT);
        let display = DisplayCST::new(&parsed.cst, &source.content, &parsed.lexed);
        println!("{}", display);
    }

    if session.has_errors() {
        for diagnostic in session.diagnostics() {
            eprintln!("{}\n", diagnostic.render_plain(&session));
        }
        std::process::exit(1);
    }

    let mut big_nums = BigNumInterner::new();
    let hir = lower(&project, &mut big_nums, &mut session);

    if args.show_hir {
        if args.show_mir {
            println!("////////////////////////////////////////////////////////////////");
            println!("//                            HIR                             //");
            println!("////////////////////////////////////////////////////////////////");
        }
        print!("{}", DisplayHir::new(&hir, &big_nums, &session));
        if args.show_mir {
            println!("\n");
            println!("////////////////////////////////////////////////////////////////");
            println!("//                            MIR                             //");
            println!("////////////////////////////////////////////////////////////////");
        }
    }

    let mir = plank_hir_eval::evaluate(&hir);

    if args.show_mir {
        print!("{}", DisplayMir::new(&mir, &big_nums));
    }

    let mut program = plank_mir_lower::lower(&mir, &big_nums);
    let mut pass_manager = PassManager::new(&mut program);
    if args.already_ssa {
        pass_manager.run_legalize().expect("illegal IR pre-ssa");
    } else {
        pass_manager.run_ssa_transform();
    }
    if let Some(passes) = args.optimize {
        pass_manager.run_optimizations(&passes);
    }

    let mut bytecode = Vec::with_capacity(0x6000);
    sir_debug_backend::ir_to_bytecode(&program, &mut bytecode);

    print!("0x");
    for byte in bytecode {
        print!("{:02x}", byte);
    }
    println!();
}
