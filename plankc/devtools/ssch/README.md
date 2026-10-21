# ssch

Tools for the stack-scheduling canonical block database. Every subcommand accepts
`-d/--database PATH`, defaulting to `corpus/stack-scheduling-db/canonical-blocks.sqlite3`.

The `canonical_blocks` table maps
`canonical_hash -> (canonical_graph, best_schedule, best_gas_cost, manually_optimized)`. Graphs and
schedules are compact JSON using canonical operation/value IDs and block-local spill slots.

## bench

Schedules every canonical graph, replays and validates each schedule, and compares the total gas
with the best-known schedules for the entire corpus and the `manually_optimized` subset.

```bash
cargo run --release -p ssch -- bench
cargo run --release -p ssch -- bench --effort 8192 -d /path/to/partition.sqlite3
cargo run --release -p ssch -- bench --save
```

The database is opened read-only unless `--save` is given, which writes every schedule cheaper than
the best known back to the database. An invalid schedule stops the run and prints its hash, graph,
stack trace, and validation error. The score is `best-known total / local total`; percentile deltas
are the nearest-rank values of `best-known gas - local gas` per graph.

## inspect

Renders a canonical graph as pseudo-SIR and replays its best-known schedule, showing the stack
(top first, right-aligned) after every operation.

```bash
cargo run -p ssch -- inspect b456edeffb54
cargo run -p ssch -- inspect --random
```

Full hashes, `ssb1:`-prefixed hashes, and unique hash prefixes are accepted. A `_f` suffix marks a
flippable graph operation; an `f` suffix marks a schedule step executing it with its first two
inputs flipped.

## submit

Validates a schedule and saves it if it is cheaper than the best known. Exits non-zero on rejection.

```bash
cargo run -p ssch -- submit b456edeffb54 op2 op1 op3 op0
cargo run -p ssch -- submit HASH <<'EOF'
dup3
op0
op1f
EOF
```

Accepted operations are `swapN`, `dupN`, `pop`, `opN`, and `opNf`. Spill slots are block-local,
numbered from zero, and written as `storeN` and `loadN`; stores must introduce slots in order.
`swapN` and `dupN` use the usual one-based EVM display depths. Updates run in a short SQLite
transaction that only writes if the schedule is still cheaper, so concurrent submissions and
`bench --save` runs retain the lowest cost.
