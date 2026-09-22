# Stack scheduler research loop

Minimal Bun/TypeScript runner using the installed `pi` CLI. No container or SDK dependency.
Research uses `openai-codex/gpt-6-astra` at low reasoning; a fresh judge uses high.
Existing pi authentication is reused. Start commands from `plankc` so Bun loads `.env`.
`TYPESAFE_API_KEY` is inherited by research; never commit `.env`.

## Setup: deliberate archive handoff

```sh
# Stop all corpus writers FIRST; leave them stopped through activation.
bun scripts/ssch-research/run.ts prepare /absolute/private/directory --writers-stopped
```

The directory must be new and outside `plankc`. Preparation:

- Creates a consistent SQLite full backup (including committed WAL data) and checks integrity.
- Creates separate, fresh 70/15/15 research/validation/final-test DBs without mutating originals.
- Groups identical canonical graphs and uses a recorded random seed. The DB has no source-family
  provenance; nonidentical relatives may cross partitions. Inspect `split-summary.json` for balance.
- Writes checksums and `ARCHIVE-CHECKLIST.md`, then **stops**.

Upload and verify the full backup, final-test DB, manifest, and summary. Archive/remove the known
original DBs, sidecars, CSV exports, older backups, and any additional copies you find as listed in
that checklist. If any writer ran after preparation, take a fresh snapshot and repeat archive verification before deleting anything. This runner does not upload or delete your original data.
Neither uploading nor deleting just `final-test.sqlite3` is sufficient: the full DB contains it too.

After removing local full-data/final-test copies:

```sh
bun scripts/ssch-research/run.ts activate /absolute/private/directory --archive-confirmed
```

Activation verifies removal of known copies and installs the research-only DB in the ordinary corpus
location. Keep private validation outside the project. Do not regenerate the full corpus during research.
The final-test archive stays offline until your independent final verification.

## Run / resume

Commit the runner, benchmark, and effort-API changes first. No implementation commits are made by setup.

```sh
bun scripts/ssch-research/run.ts run /absolute/private/directory
```

A dedicated worktree/branch starts at HEAD. The active development checkout is not changed. The runner
creates candidate commits and retains refs for rejected candidates too. `state.json` records the accepted
champion and effort settings. Up to 20 completed attempts; stop after 3 consecutive rejections.

First Ctrl-C requests a stop after the current round. Second Ctrl-C terminates the active pi process
group. Rerun the same command to resume the saved phase and inspect existing notes. Pi owns API retries;
exhausted retries or infrastructure failures pause without counting a rejection. A crashed process can
leave `.runner-lock`; remove it **only after confirming no runner/agent/benchmark is still running**.
Never run two instances against the same directory. Scope violations pause for manual inspection.

Research edits only scheduler implementation/tests, not graph semantics, gas costs, correctness validators,
or the benchmark. Each fresh session reads previous research notes and sanitized judge feedback. Notes
live under the experiment's `plankc/tmp/ssch-research/round-N`; private judge logs/evidence live outside
that worktree. The separation is prompt-based, not a security boundary: do not search private siblings,
other sessions, full-data backups, or source corpora for validation examples.

## Evaluation

```sh
cargo build --release -p sir-stack-scheduling-db-bench
# Locate target_directory with cargo metadata --no-deps --format-version 1.
<target_directory>/release/sir-stack-scheduling-db-bench --evaluate /path/to/research.sqlite3 4000
```

`--evaluate DATABASE MAX_CANDIDATES` is read-only, validates every schedule, and emits aggregate JSON.
Effort is a positive deterministic candidate budget, not a wall-clock cutoff. The legacy invocation
**without arguments still writes improvements to its DB**; never use that mode in the research loop.

Research selects three efforts for approximately 5s / 20s / 120s whole-process runtime on its visible
set. Judge runs tests and a couple of benchmark measurements on research and private validation, using
its shell tools and prompt—not a special evaluation tool. Compilation is excluded from timing. Judge
uses discretion for noise and validation slowdown. It compares against a separately built champion at
comparable budgets, prioritizing max-effort gas, then high, then baseline, and rejects unjustified
complexity. Candidate validation gas must not increase with effort; ties are fine. Current algorithm
monotonicity is not assumed: it must be measured. Effort does not promise exhaustive convergence.

Proposal: `{"efforts":[4000,16000,96000],"summary":"..."}`.
Private verdict: `{"accept":true,"feedback":"research-safe aggregates/rationale","evidence":"private checks/measurements","championEfforts":[4000,16000,96000]}`.
Calibrated champion efforts persist even when the candidate is rejected.
The judge owns correctness and metric decisions; the runner handles lifecycle, scope checks, commits,
and verdict consumption. Logs retain exact prompts, pi events/usage, and evidence. No fixed spend cap.

## Jev

The included `typesafe-skill.md` comes from the supplied TypeSafe skill. Read its live documentation.
The helper pins `jev-1.13.0` and saves request, response, usage, latency, and retry count:

```sh
bun scripts/ssch-research/jev.ts request.json result.json
```

A request contains `state` and `questions`, e.g.:

```json
{"state":"A candidate swaps two already-correct slots.","questions":{"useful":{"type":"noul","instructions":"Does this appear useful for reducing stack rearrangement?"}}}
```

Only training data/source snippets may be sent. Jev is an optional research signal for offline deterministic
heuristics, not the correctness oracle or a compiler runtime dependency. Each round records helpful,
unhelpful, inconclusive, or not-used findings; compare experiments to a non-Jev baseline.

## Tests

```sh
bun test scripts/ssch-research
(cd scripts/ssch-research && bun install && bun run typecheck)
cargo nextest run -p sir-stack-scheduling -p sir-stack-scheduling-db-bench
```
