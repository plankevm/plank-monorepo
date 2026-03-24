use clap::Parser;
use plank_driver::Driver;
use plank_hir::display::DisplayHir;
use plank_mir::display::DisplayMir;
use plank_parser::cst::display::DisplayCST;
use plank_session::SourceId;
use plank_source::source_fs::RealFs;
use sir_passes::{OPTIMIZE_HELP, parse_optimizations_string};
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
    let mut driver = Driver::new(&RealFs);

    if let Some(name) = &args.module_name {
        let root = match &args.module_root {
            Some(root) => PathBuf::from(root),
            None => Path::new(&args.file_path)
                .parent()
                .expect("file path has no parent directory")
                .to_path_buf(),
        };
        driver.register_module(name, root);
    }
    for (name, path) in &args.deps {
        driver.register_module(name, path.clone());
    }

    let project = match driver.load_project(Path::new(&args.file_path)) {
        Some(project) => project,
        None => {
            driver.render_diagnostics_and_exit();
        }
    };

    if args.show_cst {
        let parsed = &project.parsed_sources[SourceId::ROOT];
        let source = driver.session.get_source(SourceId::ROOT);
        let display = DisplayCST::new(&parsed.cst, &source.content, &parsed.lexed);
        println!("{}", display);
    }

    let hir = driver.lower_hir(&project);

    if args.show_hir {
        if args.show_mir {
            println!("////////////////////////////////////////////////////////////////");
            println!("//                            HIR                             //");
            println!("////////////////////////////////////////////////////////////////");
        }
        print!("{}", DisplayHir::new(&hir, &driver.big_nums, &driver.session));
        if args.show_mir {
            println!("\n");
            println!("////////////////////////////////////////////////////////////////");
            println!("//                            MIR                             //");
            println!("////////////////////////////////////////////////////////////////");
        }
    }

    let mir = driver.evaluate_hir(&hir);

    if args.show_mir {
        print!("{}", DisplayMir::new(&mir, &driver.big_nums));
    }

    if driver.session.has_errors() {
        driver.render_diagnostics_and_exit();
    }

    let bytecode = driver.emit_bytecode(&mir, args.already_ssa, args.optimize.as_deref());

    print!("0x");
    for byte in bytecode {
        print!("{:02x}", byte);
    }
    println!();
}
