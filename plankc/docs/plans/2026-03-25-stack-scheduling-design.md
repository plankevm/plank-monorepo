# Stack Scheduling Algorithm Design

## Problem

The debug-backend currently lowers SIR to EVM bytecode by storing all locals in memory.
Every operation results in a `PUSH addr → MLOAD` per input and `PUSH addr → MSTORE` per output.
This produces bloated bytecode with excessive memory operations.

Related: [#75 — SIR: Design & Implement Stack Shuffling with Spilling in Backend](https://github.com/plankevm/plank-monorepo/issues/75)

## Goal

Replace the naive memory-based lowering with a stack-aware codegen that keeps values on the
EVM stack when possible, minimizing DUP/SWAP operations. Memory is used only as a spill target
when the stack exceeds a configurable depth threshold.

### Priorities

1. Correctness
2. Compile-time speed
3. Codegen quality (minimize DUP/SWAP, not stack depth)

### Non-goals for V1

- Operation reordering
- Cross-block stack contracts
- Rematerialization
- Optimal scheduling

## Techniques Catalog

The design draws from these known approaches. All are compatible and sit at different layers.

| Technique | Role | Version |
|-----------|------|---------|
| Abstract stack machine | Tracks stack state symbolically, emits DUP/SWAP/load/spill | V1 |
| Linear scan mindset | Walk operations linearly, track active set of stack-resident values | V1 |
| Next-use / Bélády heuristic | Evict value with farthest next-use when stack is full | V1 |
| Permutation minimization (cycle decomposition) | Emit minimal SWAP sequence when rearranging stack | V2 |
| Peephole optimization | Post-pass cleanup of redundant SWAP/DUP/POP patterns | V2 |
| Topological sort + heuristics | Reorder operations to reduce stack pressure | V3 |
| Pebble game / register sufficiency | Determine if a block can evaluate without spills | V3 |
| Lazy code motion (ALAP bias) | Defer value production to reduce live ranges | V3 |
| Scheduling heuristics: max fanout first, closest next-use, minimize stack growth | Priority functions for topological sort | V3 |
| Rematerialization | Recompute cheap values instead of spilling | V4 |
| DP for small regions | Optimal scheduling for small basic blocks | V4 |
| Cross-block stack contracts | Keep values on stack across block boundaries | V4 |

### Heuristic tensions

- **ALAP vs max fanout first**: ALAP says produce late to reduce stack depth; max fanout says
  produce early so consumers can DUP cheaply. Resolved by priority weighting in the scheduling
  heuristic, not a conflict.
- **DP vs heuristic scheduling**: Alternatives for the same layer, dispatched by block size.

## Architecture

```
┌─────────────────────────────────────────────────┐
│  SIR Basic Block                                │
└──────────────────────┬──────────────────────────┘
                       │
         ┌─────────────▼──────────────┐
         │  Liveness Analysis         │  computes per-local live intervals,
         │                            │  next-use distances
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────┐
         │  Sufficiency Analysis      │  pebble game: can this block
         │  (stub until V3)           │  evaluate without spilling?
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────┐
         │  Operation Scheduler       │  V1: identity (no reordering)
         │  (stub until V3)           │  V3+: topological sort + heuristics
         │                            │  V4: DP for small regions
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────┐
         │  Abstract Stack Machine    │  walks scheduled operations,
         │                            │  tracks stack state symbolically,
         │  ┌───────────────────┐     │  emits EVM ops via assembler
         │  │ Stack Shuffler    │     │
         │  │ (naive until V2)  │     │  V2+: cycle decomposition
         │  ├───────────────────┤     │
         │  │ Eviction Policy   │     │  V1: next-use spill to memory
         │  │                   │     │  V4+: + rematerialization
         │  └───────────────────┘     │
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────┐
         │  Peephole Optimizer        │  redundant SWAP/DUP/POP elimination
         │  (stub until V2)           │
         └─────────────┬──────────────┘
                       │
         ┌─────────────▼──────────────┐
         │  Assembler (existing)      │  unchanged
         └────────────────────────────┘
```

## Module Structure

```
sir/crates/debug-backend/src/
├── lib.rs                      Translator (updated to use scheduler)
├── operations.rs               Replaced by stack_scheduler/machine.rs
├── static_memory_layout.rs     Kept — used for spill addresses
│
└── stack_scheduler/
    ├── mod.rs                  Orchestrates the pipeline
    ├── stack_machine.rs        Abstract stack machine
    ├── eviction.rs             Eviction policy trait + next-use implementation
    ├── analyses/
    │   ├── mod.rs
    │   ├── liveness.rs         Per-local next-use analysis
    │   └── spill.rs            Pebble game analysis (stub)
    └── transforms/
        ├── mod.rs
        ├── reorder.rs          Operation reordering (stub: identity)
        ├── shuffle.rs          Permutation minimization (stub: naive SWAPs)
        └── peephole.rs         Post-pass cleanup (stub: passthrough)
```

### Orchestration

```rust
pub fn schedule_block(...) {
    // 1. Compute liveness
    let liveness = liveness::analyze(block);

    // 2. Check sufficiency (stub: skip)
    let _sufficient = sufficiency::analyze(block, &liveness, threshold);

    // 3. Decide operation order (stub: identity)
    let order = reorder::schedule(block, &liveness);

    // 4. Run abstract stack machine — emits EVM ops
    let raw_ops = machine::execute(block, &order, &liveness, &eviction_policy);

    // 5. Peephole cleanup (stub: passthrough)
    let optimized = peephole::optimize(raw_ops);

    // 6. Emit to assembler
    emit(optimized);
}
```

## Integration with Existing Backend

### What changes

- `Translator::translate_block()` calls `stack_scheduler::schedule_block()` instead of the
  per-operation loop through `operations.rs`

### What stays the same

- `Assembler` — unchanged
- `static_memory_layout` — kept, used by eviction for spill addresses
- Control flow emission (branches, switches, jumps) — stays in `lib.rs`
- Block boundary handling — V1 keeps the memory transfer buffer

### What gets replaced

- `operations.rs` — the `OpcodeTranslator` with per-operation `load → op → store` pattern.
  Replaced by `stack_scheduler/machine.rs`.

## Block Boundary Protocol

### V1: stack empty at block entry and exit

- **Block entry**: existing `emit_transfer_basic_block_outputs` copies block inputs from transfer
  buffer into local memory slots. Stack machine loads them onto the stack on first use.
- **Block exit**: stack machine flushes any block outputs still on stack back to memory. Existing
  `emit_copy_for_basic_block_inputs` copies from local memory slots into transfer buffer.
- **Branch/switch conditions**: if condition local is on stack, consume directly; otherwise load.
- **Stack state**: empty at entry, empty at exit. No cross-block coordination.

### Future (V4+): cross-block stack contracts

Define expected stack layout at block entry/exit so values can remain on stack across transitions.

## V1 Scope

Full intra-block stack scheduling:

- **Liveness**: compute definition point, all uses, last use for every local in the block
- **Stack tracking**: abstract stack machine maintains symbolic stack state (which locals are where)
- **Input resolution**: for each operation input —
  - On stack + last use → consume (no DUP)
  - On stack + used again later → DUP
  - Not on stack → load from memory
- **Output handling**: keep on stack, don't store to memory unless spilling
- **Spilling**: when stack depth exceeds configurable threshold, evict value with farthest next-use
- **No reordering**: operations processed in original SIR order

### V1 stubs

- `sufficiency.rs` → returns "unknown"
- `reorder.rs` → returns original order
- `shuffler.rs` → naive SWAPs (linear search for needed value, swap to top)
- `peephole.rs` → passthrough

### Testing

Correctness verified by existing Foundry-based EVM execution tests (`sir-solidity-diff-tests/`).
These tests compile SIR → bytecode, deploy to EVM, and compare outputs against reference
Solidity contracts. They verify behavior, not bytecode shape.

## Version Roadmap

| Version | Adds | Modules |
|---------|------|---------|
| V1 | Full intra-block scheduling. Liveness, stack tracking, DUP for reuse, next-use spilling. | `liveness`, `machine`, `eviction` |
| V2 | Permutation minimization (cycle decomposition). Peephole cleanup. | `shuffler`, `peephole` |
| V3 | Operation reordering (topological sort + heuristics). Sufficiency analysis. | `reorder`, `sufficiency` |
| V4 | Rematerialization. DP for small regions. Cross-block stack contracts. | `eviction`, `reorder`, block boundary protocol |
