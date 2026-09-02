mod corpus;
mod database;
mod inline_constants;
mod model;
mod pipeline;
mod runner;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Build a deduplicated stack-scheduling database from a SIR corpus")]
struct Args {
    /// A .sir file or directory recursively containing .sir files.
    #[arg(default_value_os_t = default_corpus_path())]
    input: PathBuf,

    /// Directory receiving blocks.csv and canonical-blocks.csv.
    #[arg(default_value = "tmp/stack-scheduling-db")]
    output_directory: PathBuf,
}

fn default_corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../stack-scheduling-bench/corpus")
}

fn main() {
    let args = Args::parse();
    runner::run(runner::RunConfig { input: args.input, output_directory: args.output_directory });
}
