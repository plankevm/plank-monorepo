# Stack scheduling algorithm

Input: one basic block as an operation graph (`OpGraph`): block inputs on the stack (top
first), operations reading and producing values, effect ordering edges, and either a required
output stack layout (`ShuffleToOutputs`) or none. Output: a sequence of `dupN`, `swapN`, `pop`,
`store`/`load` (spills to static memory) and operations. Cost: swap/dup/pop 3 gas, store 9,
load 6, operations 0. Only stack depth 16 is reachable by `dup`/`swap`; anything deeper
must be spilled.

Entry point: `scheduler::schedule_with_effort`. Effort 0 is the compiler's fast path (one
greedy pass on the tree graph, steps 2 and 3 with no search).

## 0. Time budget

The per-graph budget is `effort' * 400ns` with `effort' = effort + effort² / 30000`. At low
effort every graph uses its full budget. At high effort most small graphs stop early (solved
exactly or search stagnates), so the remaining graphs get more time per unit of effort and
the total stays close to `effort` milliseconds for ~1,500 graphs.

## 1. Exact search for small graphs (`exact.rs`)

For graphs with at most 10 operations, at most 12 boundary values and no need to spill, run A*
over states (stack contents, set of completed operations). Moves are swap/dup/pop (unit
cost), and executing a ready operation whose operands are already on top is free. The
heuristic is admissible: copies each live value still lacks, plus surplus copies that must be
popped, plus a permutation bound at the goal. A first run forbids duplicating a value beyond
its remaining uses, which keeps the search small. A second run allows surplus dups, capped at
the first run's move count. The two runs share half of the budget. If either succeeds, its
schedule is returned.

## 2. Greedy scheduler steered by a policy (`greedy_schedule`)

A `Policy` is a genome:
- `priority[op]`: among the ready operations (all predecessors done) the smallest priority runs
  next.
- `flips[op]` for flippable (commutative) operations: Straight, Flipped, or Cheapest (try both
  orientations and keep the one that needs cheaper operand preparation).
- `pre_spill: u8`: store the top `k` block inputs to memory before anything else. Deep blocks
  often pass their top inputs through to deep output slots. Storing them at once and loading
  them when an operation or the final layout needs them beats dragging them along past reach.
  The search only changes it for blocks with at least 14 inputs, up to 8.
- `pre_shuffle`: permute the remaining inputs once up front into consumption order (inputs used
  by operations in policy order, then pass-through inputs in output order).

Each operation's operands are brought to the top by the greedy intra-op scheduler
(`greedy_intra_op_scheduler`): dup the operands that are still needed later, swap last uses
into place, and spill or load when values are out of reach. At the end, `greedy_shuffler`
permutes the stack into the output layout, popping dead values and loading spilled ones.

## 3. Policy search (`search_policy`)

Hill climbing with iterated-local-search restarts:
- Mutations: swap two priorities; move an op just before another; random priority; cycle a flip
  choice; toggle pre_shuffle (1/16); change pre_spill by ±1 (1/16, deep blocks only). Each
  candidate gets 1–2 mutations.
- A candidate is evaluated by running the greedy scheduler while counting gas only. It is
  aborted as soon as it costs more than the current genome. Equal cost is accepted, so the
  search can drift across plateaus.
- After `max(2000, 400 * ops)` candidates without improvement, restart from the best genome
  with `3 + ops/8` random mutations. Tiny graphs with fewer than 4 choices just stop.

The search runs twice:
1. On the tree graph (`treegraph`, after Park et al.): single-use chains are contracted into
   trees with a fixed internal order, which shrinks the search space. It gets half the effort.
   The result is expanded back to the original operations.
2. On the untransformed graph, seeded with the tree winner's order, flips, pre_shuffle and
   pre_spill. Tree contraction sometimes rules out the best order. It gets the other half.

The cheaper of the two schedules is kept. The winner's final shuffle is redone with the
slower, thorough shuffle strategies.

## 4. Polish (`polish.rs`)

Every maximal run of swap/dup/pop between two other instructions moves the stack from one
exact state to another. Each run is re-solved with the shuffler, and the shorter sequence is
kept.

The result is validated (`validation.rs`) before it is returned.
