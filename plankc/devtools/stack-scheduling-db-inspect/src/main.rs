mod database;
mod graph;
mod model;
mod render;

use clap::Parser;
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(about = "Display a canonical stack-scheduling graph and its best known schedule")]
struct Args {
    /// Canonical hash, with or without the ssb1: prefix.
    #[arg(required_unless_present = "random", conflicts_with = "random")]
    hash: Option<String>,

    /// Pick a random canonical block instead of specifying a hash.
    #[arg(long)]
    random: bool,

    /// Database directory or canonical-blocks.csv path.
    #[arg(short, long, default_value = "tmp/stack-scheduling-db")]
    database: PathBuf,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<String, String> {
    let entry = if args.random {
        database::random(&args.database)?
    } else {
        database::find(
            &args.database,
            args.hash.as_deref().expect("hash is required unless --random is present"),
        )?
    };
    let source_blocks_text = render::source_blocks(&entry.source_blocks);
    let graph = graph::Graph::from_representative(entry.graph)?;
    let graph_text = render::graph(&graph);
    let schedule_text = render::schedule(&graph, &entry.schedule)?;
    Ok(format!(
        "hash: {}\n\n{source_blocks_text}\n\ngraph:\n{graph_text}\n\nbest schedule (gas: {}):\n{schedule_text}",
        entry.canonical_hash, entry.gas_cost
    ))
}
