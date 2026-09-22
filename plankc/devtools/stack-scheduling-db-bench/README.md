# Stack scheduling database benchmark

Runs the current stack scheduler over every canonical graph in
`corpus/stack-scheduling-db`, compares its total gas with the best-known schedules, and saves any
improvements back to `canonical-blocks.sqlite3`.

Improvements use the submitter's conditional SQLite update, preserving concurrent submissions.
Statistics compare against the baseline snapshot loaded at startup. The report includes separate
comparisons for the entire corpus and for the subset whose `manually_optimized` flag records that
it has already been processed by the LLM runner.

```bash
cargo run --release -p sir-stack-scheduling-db-bench
```

For non-mutating research evaluation with an explicit positive search-candidate budget:

```bash
cargo run --release -p sir-stack-scheduling-db-bench -- --evaluate /path/to/partition.sqlite3 4000
```

This mode opens the database read-only and emits JSON containing graph count, effort, total gas,
scheduler-only seconds, and elapsed evaluation seconds. Measure the built binary externally for
whole-process timing; Cargo compilation is not part of the scheduling budget. The no-argument mode
above retains its existing write-back behavior.

Every generated schedule is replayed and validated. An invalid schedule stops the run immediately
and prints its hash, graph, stack trace, and validation error. The score is
`best-known total / local total`; percentile deltas are the nearest-rank values of
`best-known gas - local gas` for each graph.
