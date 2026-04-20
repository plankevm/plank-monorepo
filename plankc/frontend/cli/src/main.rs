#[cfg(test)]
use plank_test_utils as _;
#[cfg(test)]
use tempfile as _;

use clap::{Parser, Subcommand};
use owo_colors::OwoColorize;
use plank_driver::Driver;
use plank_hir::display::DisplayHir;
use plank_mir::display::DisplayMir;
use plank_parser::cst::display::DisplayCST;
use plank_session::SourceId;
use plank_source::source_fs::RealFs;
use sir_passes::{OPTIMIZE_HELP, parse_optimizations_string};
use std::{
    path::{Path, PathBuf},
    process,
};

pub fn cli_error_and_exit(message: impl Into<String>) -> ! {
    anstream::eprintln!("{}: {}", "error".red(), message.into());
    process::exit(1)
}

const VERSION: &str = match option_env!("PLANK_VERSION") {
    Some(v) => v,
    None => "dev",
};

#[derive(Parser)]
#[command(name = "plank", about = "Plank compiler frontend", version = VERSION)]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// Compile a Plank project
    Build(BuildArgs),
    /// Open Plank documentation in the browser
    Doc {
        /// Topic to open (e.g., 'comptime', 'getting-started')
        topic: Option<String>,
    },
}

#[derive(Parser)]
struct BuildArgs {
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
    let cli = Cli::parse();
    let plank_dir = resolve_plank_dir();

    match cli.action {
        Action::Build(args) => build(plank_dir, args),
        Action::Doc { topic } => {
            let doc_dir = plank_dir
                .unwrap_or_else(|| cli_error_and_exit("neither $PLANK_DIR or $HOME set"))
                .join("share/doc");
            doc(doc_dir, topic);
        }
    }
}

fn resolve_plank_dir() -> Option<PathBuf> {
    std::env::var("PLANK_DIR")
        .or_else(|_| std::env::var("HOME").map(|home| format!("{}/.plank", home)))
        .ok()
        .map(PathBuf::from)
}

fn doc(doc_dir: PathBuf, topic: Option<String>) {
    let file = match &topic {
        Some(t) => doc_dir.join(format!("{t}.html")),
        None => doc_dir.join("index.html"),
    };

    if !file.exists() {
        if let Some(t) = &topic {
            cli_error_and_exit(format!(
                "no docs found for '{t}'. Run 'plank doc' to browse all docs."
            ));
        } else {
            anstream::eprintln!(
                "{}: docs not found (searched for {file:?}), likely not installed.",
                "error".red()
            );
            anstream::eprintln!(
                "{}: Install docs with plankup, the Plank installer",
                "help".bright_blue()
            );
            anstream::eprintln!(
                "{}: See https://github.com/plankevm/plank-monorepo for installation instructions",
                "note".bright_blue()
            );
            std::process::exit(1);
        }
    }

    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    process::Command::new(opener)
        .arg(&file)
        .status()
        .unwrap_or_else(|_| cli_error_and_exit(format!("`{opener}` failed to open documentation")));
}

fn build(plank_dir: Option<PathBuf>, args: BuildArgs) {
    let mut driver = Driver::new(&RealFs);

    if let Some(name) = &args.module_name {
        let root = match &args.module_root {
            Some(root) => PathBuf::from(root),
            None => Path::new(&args.file_path)
                .parent()
                .unwrap_or_else(|| {
                    cli_error_and_exit(format!(
                        "{:?} has no parent directory to use as module root{}",
                        args.file_path, ", omit --module-name or specify --module-root",
                    ))
                })
                .to_path_buf(),
        };
        driver.register_module(name, root);
    }

    if !args.deps.iter().any(|(name, _)| name == "std")
        && let Some(std_path) = plank_dir.map(|dir| dir.join("stdlib")).filter(|p| p.is_dir())
    {
        driver.register_module("std", std_path);
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
