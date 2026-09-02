# Stack scheduling corpus benchmark

The checked-in corpus is under [`corpus/`](corpus/). Running the benchmark without arguments uses
that corpus and writes `tmp/stack-scheduling.csv` from the workspace root.

```bash
cargo run --release -p sir-stack-scheduling-bench
```

A specific SIR file or external corpus and output path can be supplied positionally:

```bash
cargo run --release -p sir-stack-scheduling-bench -- path/to/input.sir tmp/stats.csv
```

Use `--print-canonicalized` to print the exact prepared SIR passed to stack scheduling.

Generate plots and print linear regressions with:

```bash
devtools/stack-scheduling-bench/plot.py \
  tmp/stack-scheduling.csv \
  tmp/stack-scheduling-plots
```

The implementation is separated by responsibility:

- `src/corpus.rs`: corpus discovery and loading
- `src/runner.rs`: corpus iteration and pipeline execution
- `src/pipeline.rs`: benchmark pipeline definition
- `src/collection.rs`: per-block measurements and CSV output
- `src/inline_constants.rs`: benchmark-specific constant inlining pass and tests
- `plot.py`: regressions and plots
