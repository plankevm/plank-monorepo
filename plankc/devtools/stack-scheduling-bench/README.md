# Stack scheduling corpus benchmark

The default corpus is under the gitignored workspace directory `corpus/stack-scheduling`. Running
the benchmark without arguments uses that corpus and writes `tmp/stack-scheduling.csv` from the
workspace root.

```bash
cargo run --release -p sir-stack-scheduling-bench
```

A specific SIR file or external corpus and output path can be supplied positionally:

```bash
cargo run --release -p sir-stack-scheduling-bench -- corpus/my-input tmp/stats.csv
```

Use `--print-canonicalized` to print the exact prepared SIR passed to stack scheduling.

Generate plots and print linear regressions with:

```bash
devtools/stack-scheduling-bench/plot.py \
  tmp/stack-scheduling.csv \
  tmp/stack-scheduling-plots
```

The implementation is separated by responsibility:

- `src/runner.rs`: corpus iteration and pipeline execution
- `src/pipeline.rs`: benchmark scheduling pipeline
- `src/collection.rs`: per-block measurements and CSV output
- `../stack-scheduling-common`: shared corpus loading and SIR preparation
- `plot.py`: regressions and plots
