use crate::{collection::CsvCollector, pipeline::StackSchedulingPipeline};
use sir_stack_scheduling_common::Corpus;
use std::path::PathBuf;

pub fn run(input: PathBuf, output: PathBuf, print_pipeline_input: bool) {
    let corpus = Corpus::load(input);
    let mut collector = CsvCollector::create(output);

    for (index, entry) in corpus.entries().enumerate() {
        eprintln!("[{}/{}] {}", index + 1, corpus.len(), entry.display_path.display());
        let output = StackSchedulingPipeline::run(&entry.source, &entry.display_path);
        if print_pipeline_input {
            println!("=== {} ===\n{}", entry.display_path.display(), output.program);
        }
        collector.collect(&entry.display_path, &output.program, &output.scheduled);
    }

    collector.finish();
}
