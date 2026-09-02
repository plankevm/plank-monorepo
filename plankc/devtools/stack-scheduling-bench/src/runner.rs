use crate::{
    collection::CsvCollector,
    pipeline::{PipelineOutput, StackSchedulingPipeline},
};
use sir_stack_scheduling_common::Corpus;
use std::path::PathBuf;

pub struct RunConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub print_pipeline_input: bool,
}

pub fn run(config: RunConfig) {
    let corpus = Corpus::load(config.input);
    let mut collector = CsvCollector::create(config.output);

    for (index, entry) in corpus.entries().enumerate() {
        eprintln!("[{}/{}] {}", index + 1, corpus.len(), entry.display_path.display());
        let output = StackSchedulingPipeline::run(&entry.source, &entry.display_path);
        if config.print_pipeline_input {
            print_pipeline_input(&entry.display_path, &output);
        }
        collector.collect(&entry.display_path, &output.program, &output.scheduled);
    }

    collector.finish();
}

fn print_pipeline_input(path: &std::path::Path, output: &PipelineOutput) {
    println!("=== {} ===\n{}", path.display(), output.program);
}
