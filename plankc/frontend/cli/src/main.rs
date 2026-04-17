use clap::Parser;
use plank_driver::Driver;
use plank_hir::display::DisplayHir;
use plank_mir::display::DisplayMir;
use plank_parser::cst::display::DisplayCST;
use plank_session::SourceId;
use plank_source::source_fs::RealFs;
use sir_passes::{OPTIMIZE_HELP, parse_optimizations_string};
use std::path::{Path, PathBuf};

const VERSION: &str = match option_env!("PLANK_VERSION") {
    Some(v) => v,
    None => "dev",
};

#[derive(Parser)]
#[command(name = "plank", about = "Plank compiler frontend", version = VERSION)]
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
    let root = Path::new(&args.file_path)
        .parent()
        .expect("file path has no parent directory")
        .canonicalize()
        .expect("failed to canonicalize project root");
    let mut driver = Driver::new(&RealFs, root.clone());

    if let Some(name) = &args.module_name {
        let module_root = args.module_root.map_or(root, |p| {
            PathBuf::from(p).canonicalize().expect("failed to canonicalize module root")
        });
        driver.register_module(name, module_root);
    }
    for (name, path) in &args.deps {
        let path = path.canonicalize().expect("failed to canonicalize dependency path");
        driver.register_module(name, path);
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
        print!("{}", DisplayHir::new(&hir, &driver.values, &driver.session));
        if args.show_mir {
            println!("\n");
            println!("////////////////////////////////////////////////////////////////");
            println!("//                            MIR                             //");
            println!("////////////////////////////////////////////////////////////////");
        }
    }

    let mir = driver.evaluate_hir(&hir);

    if args.show_mir {
        print!("{}", DisplayMir::new(&mir, &driver.values, &driver.session));
    }

    if driver.session.has_errors() {
        driver.render_diagnostics_and_exit();
    }

    let bytecode = driver.emit_bytecode(&mir, args.optimize.as_deref());

    print!("0x");
    for byte in bytecode {
        print!("{:02x}", byte);
    }
    println!();
}
