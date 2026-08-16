#[cfg(test)]
use plank_test_utils as _;
#[cfg(test)]
use tempfile as _;

use clap::{Parser, Subcommand, ValueEnum};
use owo_colors::OwoColorize;
use plank_driver::{BackendKind, Driver, print_ir};
use plank_evm::EvmVersion;
use plank_hir::display::DisplayHir;
use plank_mir::{Mir, display::DisplayMir};
use plank_parser::cst::display::DisplayCST;
use plank_session::SourceId;
use plank_source::{SourceFs, source_fs::RealFs};
use sir_passes::OPTIMIZE_HELP;
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
    /// Check a Plank project
    Check(CheckArg),
    /// Open Plank documentation in the browser
    Doc {
        /// Topic to open (e.g., 'comptime', 'getting-started')
        topic: Option<String>,
    },
}

#[derive(Parser)]
struct ProjectArgs {
    file_path: String,

    #[arg(long = "module-name")]
    module_name: Option<String>,

    #[arg(long = "module-root", requires = "module_name")]
    module_root: Option<String>,

    #[arg(long = "dep", value_parser = parse_dep)]
    deps: Vec<(String, PathBuf)>,

    #[arg(long = "evm-version", value_enum, default_value_t = EvmVersionArg::Osaka)]
    evm_version: EvmVersionArg,
}

#[derive(Parser)]
struct FrontendDisplayArgs {
    #[arg(short = 'c', long = "show-cst", help = "show CST")]
    show_cst: bool,

    #[arg(long = "show-hir", help = "show HIR")]
    show_hir: bool,

    #[arg(short = 'm', long = "show-mir", help = "show MIR")]
    show_mir: bool,
}

#[derive(Parser)]
struct BackendDisplayArgs {
    #[arg(
        long = "show-sir-first",
        help = "show the selected backend IR before backend optimizations"
    )]
    show_sir_in: bool,

    #[arg(long = "show-sir-final", help = "show the selected backend IR before bytecode emission")]
    show_sir_last: bool,
}

#[derive(Parser)]
struct CheckArg {
    #[command(flatten)]
    common_args: ProjectArgs,

    #[command(flatten)]
    frontend_display_args: FrontendDisplayArgs,
}

#[derive(Parser)]
struct BuildArgs {
    #[command(flatten)]
    common_args: ProjectArgs,

    #[command(flatten)]
    frontend_display_args: FrontendDisplayArgs,
    
    #[command(flatten)]
    backend_display_args: BackendDisplayArgs,

    // backend specify
    #[arg(short = 'O', long = "optimize", help = optimize_help())]
    optimize: Option<String>,

    #[arg(long = "backend", value_enum, default_value_t = BackendArg::SirDebug)]
    backend: BackendArg,
}

impl FrontendDisplayArgs {
    fn needs_separators(&self) -> bool {
        (self.show_hir as u32)
            + (self.show_mir as u32)
            >= 2
    }
}

impl BackendDisplayArgs {
    fn needs_separators(&self) -> bool {
             (self.show_sir_in as u32)
            + (self.show_sir_last as u32)
            >= 2
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum EvmVersionArg {
    Cancun,
    Prague,
    Osaka,
}

impl From<EvmVersionArg> for EvmVersion {
    fn from(value: EvmVersionArg) -> Self {
        match value {
            EvmVersionArg::Cancun => EvmVersion::Cancun,
            EvmVersionArg::Prague => EvmVersion::Prague,
            EvmVersionArg::Osaka => EvmVersion::Osaka,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BackendArg {
    SirDebug,
    SirRelease,
    Sona,
}

impl From<BackendArg> for BackendKind {
    fn from(value: BackendArg) -> Self {
        match value {
            BackendArg::SirDebug => BackendKind::SirDebug,
            BackendArg::SirRelease => BackendKind::SirRelease,
            BackendArg::Sona => BackendKind::Sona,
        }
    }
}

fn optimize_help() -> String {
    format!(
        "{OPTIMIZE_HELP}\n\n\
        Sonatina backend optimization levels: O0, O1, Os, O2. Default is O0.\n\
        Examples: --backend sona -O1, --backend sona -O2"
    )
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
        Action::Check(args) => check(plank_dir, args),
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

fn check(plank_dir: Option<PathBuf>, args: CheckArg) {
    let mut driver = Driver::new(&RealFs);

    let common_args = args.common_args;
    let frontend_display_args = args.frontend_display_args;
    register_modules(&mut driver, &common_args, plank_dir);
    if run_frontend(&mut driver, &common_args, &frontend_display_args).is_none() {
        driver.render_diagnostics_and_exit()
    }
}

fn build(plank_dir: Option<PathBuf>, args: BuildArgs) {
    let mut driver = Driver::new(&RealFs);
    let common_args = args.common_args;
    let frontend_display_args = args.frontend_display_args;
    let backend_display_args =args.backend_display_args;
    register_modules(&mut driver, &common_args, plank_dir);

    match run_frontend(&mut driver, &common_args, &frontend_display_args) {
        None => driver.render_diagnostics_and_exit(),
        Some(mir) => {
            let bytecode = driver
                .emit_bytecode_with_backend(
                    &mir,
                    args.optimize.as_deref(),
                    backend_display_args.needs_separators(),
                    backend_display_args.show_sir_in,
                    backend_display_args.show_sir_last,
                    args.backend.into(),
                )
                .unwrap_or_else(|err| cli_error_and_exit(err));

            println!("{:#}", alloy_primitives::hex::display(bytecode));
        }
    };
}

fn register_modules<F: SourceFs>(
    driver: &mut Driver<F>,
    common_args: &ProjectArgs,
    plank_dir: Option<PathBuf>,
) {
    if let Some(name) = &common_args.module_name {
        let root = match &common_args.module_root {
            Some(root) => PathBuf::from(root),
            None => Path::new(&common_args.file_path)
                .parent()
                .unwrap_or_else(|| {
                    cli_error_and_exit(format!(
                        "{:?} has no parent directory to use as module root{}",
                        common_args.file_path, ", omit --module-name or specify --module-root",
                    ))
                })
                .to_path_buf(),
        };
        driver.register_module(name, root);
    }

    let std_path = common_args
        .deps
        .iter()
        .find_map(|(name, path)| (name == "std").then_some(path.clone()))
        .or_else(|| plank_dir.map(|dir| dir.join("stdlib")).filter(|p| p.is_dir()));

    if let Some(std_path) = std_path {
        driver.register_std(std_path);
    }

    for (name, path) in &common_args.deps {
        if name == "std" {
            continue;
        }
        driver.register_module(name, path.clone());
    }
}

fn run_frontend<F: SourceFs>(
    driver: &mut Driver<F>,
    common_args: &ProjectArgs,
    frontend_display_args: &FrontendDisplayArgs,
) -> Option<Mir> {
    let project = match driver.load_project(Path::new(&common_args.file_path)) {
        Some(project) => project,
        None => {
            driver.render_diagnostics_and_exit();
        }
    };

    if frontend_display_args.show_cst {
        let parsed = &project.parsed_sources[SourceId::ROOT];
        let source = driver.session.get_source(SourceId::ROOT);
        let display = DisplayCST::new(&parsed.cst, &source.content, &parsed.lexed);
        println!("{}", display);
    }

    let hir = driver.lower_hir(&project);

    if frontend_display_args.show_hir {
        print_ir(
            "HIR",
            frontend_display_args.needs_separators(),
            DisplayHir::new(&hir, &driver.values, &driver.session),
        );
    }

    let mir = driver.evaluate_hir(&hir, project.core_ops_source, common_args.evm_version.into());

    if frontend_display_args.show_mir {
        print_ir(
            "MIR",
            frontend_display_args.needs_separators(),
            DisplayMir::new(&mir, &driver.values, &driver.session),
        );
    }

    if driver.session.has_errors() { None } else { Some(mir) }
}
