use clap::Parser;
use csv as _;
use plank_core as _;
use rand as _;
use serde as _;
use serde_json as _;
use sir_stack_scheduling_db_inspect::{
    Graph, find, random, render_graph, render_schedule, render_source_blocks,
};
use std::{path::PathBuf, process::ExitCode};

#[cfg(test)]
use plank_test_utils as _;

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
        random(&args.database)?
    } else {
        find(
            &args.database,
            args.hash.as_deref().expect("hash is required unless --random is present"),
        )?
    };
    let source_blocks_text = render_source_blocks(&entry.source_blocks);
    let graph = Graph::from_representative(entry.graph)?;
    let graph_text = render_graph(&graph);
    let schedule_text = render_schedule(&graph, &entry.schedule)?;
    Ok(format!(
        "hash: {}\n\n{source_blocks_text}\n\ngraph:\n{graph_text}\n\nbest schedule (gas: {}):\n{schedule_text}",
        entry.canonical_hash, entry.gas_cost
    ))
}
