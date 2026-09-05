# Stack scheduling database inspector

Displays every source file/basic-block occurrence of a canonical hash, renders its graph as
pseudo-SIR, and replays the best known schedule while showing the stack after every operation.
Stack values are shown from top to bottom and right-aligned.

```bash
cargo run -p sir-stack-scheduling-db-inspect -- \
  ssb1:b456edeffb54263bdc5e7525e9d69c976235fc5c75c11f28ea487539cd7d79d8
```

Omit the hash and select a random canonical block with `--random`:

```bash
cargo run -p sir-stack-scheduling-db-inspect -- \
  --random \
  --database corpus/stack-scheduling-db-full
```

The selected hash is printed above the graph. The default database directory is
`corpus/stack-scheduling-db`. Select another generated database or pass its `canonical-blocks.sqlite3`
directly with `--database`:

```bash
cargo run -p sir-stack-scheduling-db-inspect -- \
  b456edeffb54263bdc5e7525e9d69c976235fc5c75c11f28ea487539cd7d79d8 \
  --database corpus/stack-scheduling-db-full
```

A `_f` suffix marks a flippable graph operation. An `f` suffix marks a schedule step that executes
the operation with its first two inputs flipped.
