# Stack scheduling database builder

This tool runs the current stack-scheduling pipeline over the benchmark corpus and writes a
CSV database under the gitignored workspace directory `corpus/stack-scheduling-db`:

- `blocks.csv`: `(file, block_id) -> canonical_hash`
- `canonical-blocks.csv`: `canonical_hash -> (canonical_graph, best_schedule, best_gas_cost)`

The graph and schedule columns contain compact JSON. Graph operation and value IDs are canonical
IDs, and spill allocation IDs in schedules are normalized to block-local slots. The graph records
all data dependencies, non-data effect predecessors, operation arities, flippability, outputs, and
block finalization, which is sufficient to reconstruct a representative operation graph for stack
scheduling.

Run it with the default corpus at `corpus/stack-scheduling`:

```bash
cargo run --release -p sir-stack-scheduling-db
```

A SIR file or another corpus and output directory can be supplied positionally:

```bash
cargo run --release -p sir-stack-scheduling-db -- \
  corpus/my-input \
  corpus/my-scheduling-db
```

When multiple corpus blocks have the same canonical hash, the canonical table retains the schedule
with the lowest current pipeline gas cost.
