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

The per-graph budget is `effort' * 400ns` with `effort' = effort + effort² / 16000`. At low
effort every graph uses its full budget. At high effort most small graphs stop early (solved
exactly or search stagnates), so the remaining graphs get more time per unit of effort. For
~1,500 graphs the total stays below `effort` milliseconds (measured: about 0.6× at effort
1000 and an estimated 0.9× at effort 23000).

Graphs with more than 32 operations get `effort'` scaled by `ops / 32`, capped at 8× (so 64
ops get 2×, 256+ ops get 8×). Large graphs are few (about 3% of the corpus have more than 63
ops), always use their whole budget, and keep improving with more time, while small graphs
mostly finish early. On the research corpus this cost about +10% wall time and saved about
550 gas, mostly on graphs with 64–200 ops. Spending the same time on all graphs saves less.

With effort > 0, 25% of that budget (`WINDOW_BUDGET_SHARE`) is reserved for the window
re-optimization (step 5); the rest is the search budget `B` for steps 1-4.

## 0b. Parallel search

If the search budget `B` is at least 2,500 effort units (1 ms), steps 1–3 use up to 12 threads
(`SEARCH_THREADS`, capped by the available cores). Lower budgets, such as most graphs at effort
1000, run sequentially, because spawning threads costs tens of microseconds. All steps are
deadline bound, so extra threads use more cores but no extra wall time.

1. The A* of step 1 first runs alone for `0.1 B`. Easy graphs are solved here and never start
   threads.
2. If that fails:
   - For graphs with at most 16 ops, one thread restarts the A* with a deadline of `0.5 B`,
     then runs the beam search of step 1b until `B`. The other 11 threads each run the full
     step 3 (tree search, then flat search) for the remaining budget. Each gets a different
     random seed.
   - For larger graphs, all 12 threads run step 3.
3. If the A* succeeds, it sets a stop flag that ends the policy searches, and its schedule is
   returned.
4. Otherwise the search results, plus the beam schedule if there is one, are deduplicated.
   Each distinct result runs steps 4 and 5
   on its own thread: polish, then the beam schedule replaces it if cheaper, then windows
   with their own `0.25`-share deadline. The cheapest finished schedule wins. A search result
   that is slightly worse before the windows sometimes ends cheaper after them. Sequential
   runs have a single result, so they finish it on the calling thread.

On the research corpus at effort 23000 this cut gas from 163,023 to 160,791 (−1.4%). Wall
time went from 91.5 s to 96.4 s, mostly thread overhead on small graphs. Most of the gain
came from graphs with 17+ ops. Running the A* next to the searches, instead of before them,
helped graphs of 6–16 ops by about 230 gas.

Finishing every distinct search result instead of only the cheapest cut gas by about 700
more (160,509 → 159,807 in the same contended batch). It helped every size bucket and cost
about 2% wall time, which comes from waiting for the slowest finisher. Finishing the beam
schedule as a candidate of its own, rather than only swapping it in when it is cheaper
before the windows, saved about 100 more gas on graphs of 16 ops or fewer.

## 1. Exact search for small graphs (`exact.rs`)

For graphs with at most 10 operations, at most 12 boundary values and no need to spill, run A*
over states (stack contents, set of completed operations). Moves are swap/dup/pop (unit
cost), and executing a ready operation whose operands are already on top is free. The
heuristic is admissible: copies each live value still lacks, plus surplus copies that must be
popped, plus a swap bound. A value whose producer has not run yet will appear exactly once, so
it still needs `uses - 1` dups, or one pop if it has no use. A value with a single copy, no remaining readers and exactly one
output slot can only change how many values sit below it through a swap that moves it. Each
swap moves two values, so ⌈misplaced such values / 2⌉ swaps are still needed. Once all
operations are done, the bound is the larger of that and the number of misplaced positions
below the top plus one for each permutation cycle that does not contain the top (such a cycle
needs an extra swap to bring one of its members up first; only used when the goal holds no
value twice). A first run forbids duplicating a value beyond
its remaining uses, which keeps the search small. A second run allows surplus dups, capped at
the first run's move count. The two runs share `B/2`. If either succeeds, its schedule is
returned.

## 1b. Beam search (graphs slightly too large for the A*)

`exact::beam_schedule`, ≤16 ops, given 30% of what is left of `B`. Same states and moves as the
A*. Unlike the A*, it also runs on blocks with 13–16 inputs or outputs: its result is only a
candidate next to the policy searches, so a spill-free schedule that is worse than one with
spills does no harm. On the research corpus this saved about 80 gas, all on graphs of 1–10 ops
with 12–16 boundary values. Those graphs mostly need a long permutation, and the beam's thread
used to sit idle on them. Layer k = states reached with k stack moves; free op moves are closed over inside a layer
(capped at 8 × width states, since independent ready ops can interleave in exponentially many
ways). The `width` states with the smallest heuristic (ties: more ops done) are expanded into
layer k+1. A global seen-set dedupes states. Runs with width 4, 8, 16, ... until its deadline,
keeping the cheapest goal reached. Its result replaces the polished schedule of step 4 if
cheaper.

## 2. Greedy scheduler steered by a policy (`greedy_schedule`)

A `Policy` is a genome:
- `priority[op]`: among the ready operations (all predecessors done) the smallest priority runs
  next.
- `flips[op]` for flippable (commutative) operations: Straight, Flipped, or Cheapest (try both
  orientations and keep the one that needs cheaper operand preparation).
- `pre_spill: u8`: store the top `k` block inputs to memory before anything else, or pop them
  if dead (no readers, not an output). Deep blocks often pass their top inputs through to deep
  output slots. Storing them at once and loading them when an operation or the final layout
  needs them beats dragging them along past reach. The search changes it for blocks with at
  least 14 inputs (up to 8), and otherwise only over the run of dead inputs on top.
- `pre_swaps: [u8; 3]`: swaps applied after the pre-spills (0 = none, depths beyond the stack
  are skipped). They sink pass-through inputs towards their output slots while still in reach.
- `op_swaps[op]`: two swap slots of depth 1–16 (0 = none, depths beyond the stack are skipped)
  applied right before preparing the operands of `op`. Mutations only pick depths up to the
  deepest stack seen so far in the search minus one, since deeper swaps are skipped anyway. The greedy operand preparer only cares about the
  next operation. This gene lets the search park a value at the depth it is wanted later, so
  later operations and the final layout need fewer moves. On the research corpus it was the
  single largest improvement (−2.7% gas).
- `pre_shuffle`: permute the remaining inputs once up front into consumption order (inputs used
  by operations in policy order, then pass-through inputs in output order).
- `shuffle_before: Option<op>`: right before preparing the operands of this op (before its
  op-swaps), permute the whole stack with the greedy shuffler into: values read by unfinished
  operations, in policy order, that are not block outputs; then the block outputs present on
  the stack, in output order (deepest occurrence kept, each value once since the operand
  preparer needs a duplicate-free stack). Other values are popped. Many blocks are cheapest
  when one long permutation runs after an op or two has shrunk the stack, e.g. `a3d63cc9`
  (4 ops, 13 inputs: run one op, rotate 10 values, run the rest; 45 → 39 gas). Mutation: 1/16
  of the remaining, set to a random op or clear. A resumed evaluation diverges at the step whose
  op has the shuffle in either genome (and, if the candidate's, when priorities differ, since
  the layout depends on them). Not carried from the tree search to the flat search. On the
  research corpus at effort 23000: −1,344 gas (159,567 → 158,223, side by side), same wall
  time, gains in every size bucket, most on 6–32 ops. Keeping outputs at their output positions
  is what matters: putting consumed outputs in the consumption part instead (as `pre_shuffle`
  does) loses almost all of the gain, and a gene bit choosing between the two layouts is +170.
  Two shuffle slots: no gain; mutation rate 1/8: +190; 1/32: +50 (noise).

Ready operations are tracked incrementally: each operation counts its unfinished immediate
predecessors (transitive reduction of the dependency graph) and joins the ready list when the
count reaches zero. A `Cheapest` flip runs the operand preparer for both orientations on copies
of the stack and replays the cheaper one's moves.

Each operation's operands are brought to the top by the greedy intra-op scheduler
(`greedy_intra_op_scheduler`): dup the operands that are still needed later, swap last uses
into place, and spill or load when values are out of reach. At the end, `greedy_shuffler`
permutes the stack into the output layout, popping dead values and loading spilled ones.

## 3. Policy search (`search_policy`)

Hill climbing with iterated-local-search restarts:
- Mutations: toggle pre_shuffle (1/16); set or clear a pre-swap (1/16); change pre_spill by ±1
  (1/16 when allowed); set or clear shuffle_before (1/16); set one op-swap slot to a random depth or clear it (1/2 of the remaining); cycle a flip choice;
  otherwise change priorities (swap two, move an op just before another, or randomise one).
  Each candidate gets 1–2 mutations.
- A candidate is evaluated by running the greedy scheduler while counting gas only. It is
  aborted as soon as it costs more than the current genome.
- Evaluations resume instead of starting over. The search keeps a trace of the current genome's
  greedy run: for every step, the op run and the state before it (stack, spilled values, gas so
  far). A candidate with the same prelude (pre_spill, pre_swaps, pre_shuffle, and if
  pre_shuffle is on the same priorities, since the shuffle layout depends on them) picks
  ops by its own priorities while they match the trace and the op has the same op-swaps and
  flip; the greedy run then starts from the checkpoint of the first differing step. If no step
  differs the candidate costs the same as the current genome. An accepted candidate's new steps
  replace the trace from that point on. Results are identical to a full run; on graphs with
  many ops this roughly halves the work per candidate. Equal cost is accepted, so the
  search can drift across plateaus.
- After `max(2000, 400 * ops)` candidates without improvement, restart from the best genome
  with `3 + ops/8` random mutations. Tiny graphs with fewer than 4 choices just stop.

The search runs twice:
1. On the tree graph (`treegraph`, after Park et al.): single-use chains are contracted into
   trees with a fixed internal order, which shrinks the search space. It gets half the effort.
   The result is expanded back to the original operations.
2. On the untransformed graph, seeded with the tree winner's order, flips, pre_shuffle,
   pre_spill and pre_swaps. Op swaps start at 0 because tree operations do not map one-to-one
   to graph operations. Tree contraction sometimes rules out the best order. It gets the other half.

The cheaper of the two schedules is kept. The winner's final shuffle is redone with the
slower, thorough shuffle strategies.

## 4. Polish (`polish.rs`)

Every maximal run of swap/dup/pop between two other instructions moves the stack from one
exact state to another. Each run is re-solved with the shuffler, and the shorter sequence is
kept.

## 5. Window re-optimization

`exact::reoptimize_windows`, on the final schedule of any graph. Replay the schedule and record
a boundary state (stack, op just run) right after every op and every spill `store`/`load`, plus
the final state. For window sizes w = 2, 3, ..., 10 boundaries: for each boundary i, take
boundaries i and i+w (clamped to the end), skip the segment between them if it contains a
spill, and otherwise re-solve it with the A* of step 1 on a local subproblem where

- only the segment's ops run; they get local bits and the values involved local ids (block
  size does not matter), op readers and predecessors outside the window are ignored,
- the pass-through swap bound uses the end boundary's stack as the goal,
- the goal is exactly the end boundary's stack (or, for the final window of a
  `LastOpTerminates` block, just "all ops done"),
- the move limit is the segment's current number of moves, so only strictly better segments
  are returned,
- on deep stacks (either end deeper than 17) only the top 16 values are part of the problem.
  The `max(len) - 16` values below must be the same at both ends, or the window is skipped. The
  A* never touches them, and a window op that reads one of them makes the problem unsolvable.
  The 16 leaves one slot free for a dup. These windows used to be skipped. Re-solving them saved
  about 150 gas on the research corpus, mostly on blocks with 13 or more boundary values.

An improved segment is spliced in, boundaries are recomputed and the sweep restarts a little
before the changed window. Each window size sweeps until no improvement; the pass stops at its
deadline. Because both ends of a window are fixed, every replacement keeps the schedule valid
and never increases gas.

This step fixes the greedy operand preparer's myopia: e.g. preparing the operands of the last
few ops so that the final output layout falls out without extra swaps, or picking which copy of
a value to consume.

The result is validated (`validation.rs`) before it is returned.
