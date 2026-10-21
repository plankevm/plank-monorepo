mod bench;
mod database;
mod inspect;
mod stats;
mod submit;

use clap::{Args, Parser, Subcommand};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Parser)]
#[command(about = "Stack-scheduling canonical block database tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Schedule and validate every canonical block, comparing against the best-known schedules.
    Bench {
        /// Search-candidate budget passed to the scheduler; the scheduler default when omitted.
        #[arg(short, long)]
        effort: Option<u32>,

        /// Write schedules cheaper than the best known back to the database.
        #[arg(long)]
        save: bool,

        /// Print the summary as a single JSON object instead of the text report.
        #[arg(long)]
        json: bool,

        /// Append the N graphs with the largest gap to the best-known schedule.
        #[arg(long, value_name = "N")]
        worst: Option<usize>,

        /// Write a TSV row per graph (sorted by gap, largest first) to PATH.
        #[arg(long, value_name = "PATH")]
        per_graph: Option<PathBuf>,

        #[command(flatten)]
        database: DatabaseArg,
    },
    /// Display a canonical graph and replay its best-known schedule.
    Inspect {
        /// Canonical hash or unique hash prefix, with or without the ssb1: prefix.
        #[arg(required_unless_present = "random", conflicts_with = "random")]
        hash: Option<String>,

        /// Pick a random canonical block instead of specifying a hash.
        #[arg(long)]
        random: bool,

        #[command(flatten)]
        database: DatabaseArg,
    },
    /// Validate a schedule and save it if it is cheaper than the best known.
    Submit {
        /// Canonical hash or unique hash prefix, with or without the ssb1: prefix.
        hash: String,

        /// Whitespace-separated stack operations. Reads stdin when omitted.
        #[arg(value_name = "STACK_OP")]
        stack_ops: Vec<String>,

        #[command(flatten)]
        database: DatabaseArg,
    },
}

#[derive(Args)]
struct DatabaseArg {
    /// Path to the canonical-blocks SQLite database.
    #[arg(short = 'd', long = "database", default_value_os_t = default_database_path())]
    path: PathBuf,
}

fn default_database_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("ssch crate is not under the workspace devtools directory")
        .join("corpus/stack-scheduling-db/canonical-blocks.sqlite3")
}

fn main() -> ExitCode {
    let result = match Cli::parse().command {
        Command::Bench { effort, save, json, worst, per_graph, database } => {
            let options = bench::Options { effort, save, json, worst, per_graph };
            bench::run(&database.path, &options).map(|output| (output, true))
        }
        Command::Inspect { hash, random, database } => {
            inspect::run(&database.path, hash.as_deref(), random).map(|output| (output, true))
        }
        Command::Submit { hash, stack_ops, database } => {
            read_stack_ops(stack_ops).and_then(|source| {
                submit::run(&database.path, &hash, &source)
                    .map(|result| (result.output, result.accepted))
            })
        }
    };
    match result {
        Ok((output, success)) => {
            println!("{output}");
            if success { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn read_stack_ops(stack_ops: Vec<String>) -> Result<String, String> {
    if !stack_ops.is_empty() {
        return Ok(stack_ops.join(" "));
    }
    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .map_err(|error| format!("failed to read schedule from stdin: {error}"))?;
    Ok(source)
}
