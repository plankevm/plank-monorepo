mod collection;
mod pipeline;
mod runner;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Collect per-block stack scheduling statistics for a SIR corpus")]
struct Args {
    /// A .sir file or directory recursively containing .sir files.
    #[arg(default_value_os_t = default_corpus_path())]
    input: PathBuf,

    /// Destination CSV file.
    #[arg(default_value = "tmp/stack-scheduling.csv")]
    output: PathBuf,

    /// Print every program after constant inlining and critical-edge splitting.
    #[arg(long, alias = "print-pipeline-input")]
    print_canonicalized: bool,
}

fn default_corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

fn main() {
    let args = Args::parse();
    runner::run(runner::RunConfig {
        input: args.input,
        output: args.output,
        print_pipeline_input: args.print_canonicalized,
    });
}
