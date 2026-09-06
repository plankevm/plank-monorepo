mod database;
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

    /// Directory receiving blocks.csv and canonical-blocks.sqlite3.
    #[arg(default_value_os_t = default_database_path())]
    output_directory: PathBuf,
}

fn default_corpus_path() -> PathBuf {
    sir_stack_scheduling_common::workspace_corpus_path("stack-scheduling")
}

fn default_database_path() -> PathBuf {
    sir_stack_scheduling_common::workspace_corpus_path("stack-scheduling-db")
}

fn main() {
    let args = Args::parse();
    runner::run(runner::RunConfig { input: args.input, output_directory: args.output_directory });
}
