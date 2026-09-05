# Stack scheduling database submission tool

Validates a manually authored schedule for a canonical graph and replaces the database baseline
when the submitted schedule uses less gas. The default database is
`corpus/stack-scheduling-db`.

Pass stack operations as whitespace-separated arguments:

```bash
cargo run -p sir-stack-scheduling-db-submit -- \
  b456edeffb54263bdc5e7525e9d69c976235fc5c75c11f28ea487539cd7d79d8 \
  op2 op1 op3 op0
```

When no operations are given, the schedule is read from standard input, which is convenient for
multiline submissions:

```bash
cargo run -p sir-stack-scheduling-db-submit -- HASH <<'EOF'
dup3
op0
op1f
EOF
```

Accepted operations are `swapN`, `dupN`, `pop`, `opN`, and `opNf`. Spill slots are block-local,
numbered from zero, and written as `storeN` and `loadN`; stores must introduce slots in order.
`swapN` and `dupN` use the usual one-based EVM display depths.

`--database` accepts a directory or `canonical-blocks.sqlite3` path. Validation runs before a short
SQLite transaction that updates only if the schedule is still cheaper. Concurrent submissions and
benchmarks retain the lowest cost; no external lock file is needed.
