# Stack scheduling algorithm

Input: one basic block as an operation graph (block input values on the stack, operations with
operand/result values and ordering edges, block output values or "last op terminates").
Output: a sequence of `swapN`/`dupN`/`pop`, operations, and spill `store`/`load`s. Cost is gas:
swap/dup/pop 3, store 9, load 6, operations 0.

Effort `e` grants a single graph `e × 400ns` of wall time (`SEARCH_NANOS_PER_EFFORT`). Effort 0
runs only the greedy tree-graph schedule. With effort > 0 the budget is split:

- 25% (`WINDOW_BUDGET_SHARE`) is reserved for the window re-optimization at the end (step 5).
- The rest is the search budget `B` for steps 1-4.

## 1. Exact A* (small graphs)

`exact::exact_schedule`. For graphs with ≤10 ops and ≤12 block inputs/outputs. State = (stack
contents top-first, ≤17 slots; bitmask of done ops). Moves: `swap`, `dup`, `pop` cost 1, executing
a ready op whose operands are exactly on top costs 0 (flippable ops in both operand orders).
Heuristic: per produced value, the copies still missing for its remaining uses plus the surplus
copies that must be popped; when all ops are done, the number of misplaced slots below the top.
First run forbids duplicating a value beyond its remaining uses (small search space), then a
second run allows one surplus copy, bounded by the first result. If it finishes within `B/2` the
result is final. Spill-free only.

## 2. Beam search (graphs slightly too large for the A*)

`exact::beam_schedule`, ≤16 ops, given 30% of what is left of `B`. Same states and moves as the
A*. Layer k = states reached with k stack moves; free op moves are closed over inside a layer
(capped at 8 × width states, since independent ready ops can interleave in exponentially many
ways). The `width` states with the smallest heuristic (ties: more ops done) are expanded into
layer k+1. A global seen-set dedupes states. Runs with width 4, 8, 16, ... until its deadline,
keeping the cheapest goal reached. Its result only competes at the end of step 4.

## 3-4. Policy search over a greedy scheduler

`scheduler::greedy_schedule` schedules ops one at a time: among ready ops pick the one with the
smallest policy priority, prepare its operands with the greedy operand preparer
(`greedy_intra_op_scheduler`, spills when the stack gets too deep), run it, and finally shuffle
to the block outputs with `greedy_shuffler`. A `Policy` is (priority per op, flip choice per
flippable op, optional input pre-shuffle).

`search_policy` is hill climbing with restarts over policies (random swaps/moves of priorities,
flip changes, pre-shuffle toggle; equal-cost moves accepted; after stagnation, restart from the
best with a random kick). It runs first on the tree graph (`treegraph`, ops grouped into trees
whose internal order is fixed) with half the remaining budget, then on the plain op graph with
the other half, seeded with the tree winner. The cheaper schedule is kept and `polish` re-solves
every maximal run of pure stack moves between two ops with the shuffler. Then the beam result
(step 2) replaces it if cheaper.

## 5. Window re-optimization

`exact::reoptimize_windows`, on any spill-free final schedule of a graph with ≤63 ops. Replay
the schedule and record a boundary state (stack, done mask) right after every op, plus the final
state. For window sizes w = 2, 3, ..., 8 ops: for each boundary i, take boundaries i and i+w
(clamped to the end), and re-solve the segment between them with the A* of step 1 where

- only ops done at the end boundary may run (op readers outside the window are ignored),
- the goal is exactly the end boundary's stack (or, for the final window of a
  `LastOpTerminates` block, just "all ops done"),
- the move limit is the segment's current number of moves, so only strictly better segments
  are returned.

An improved segment is spliced in, boundaries are recomputed and the sweep restarts a little
before the changed window. Each window size sweeps until no improvement; the pass stops at its
deadline. Because both ends of a window are fixed, every replacement keeps the schedule valid
and never increases gas.

This step fixes the greedy operand preparer's myopia: e.g. preparing the operands of the last
few ops so that the final output layout falls out without extra swaps, or picking which copy of
a value to consume.

All results are checked by `validation::validate`.
