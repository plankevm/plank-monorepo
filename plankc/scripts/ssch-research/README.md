# Stack scheduler research loop

Minimal Bun/TypeScript runner using the installed `pi` CLI. No container or SDK dependency.
Research uses `openai-codex/gpt-6-sol` at high reasoning; a fresh judge uses `openai-codex/gpt-6-astra` at high.
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
set. After the initial champion has been measured, effort settings are frozen for this run. A verified
private cache records the accepted champion commit, research/validation DB checksums, validation gas
and research wall times; for an existing run it is bootstrapped from the last accepted round's saved
candidate measurements (without rerunning the champion). Missing/mismatched cache pauses, not guesses.
The judge runs tests and TWO repetitions at the three fixed settings for the candidate on research and
validation (12 benchmark executions total); a third repetition of a candidate setting is allowed only
to resolve timing noise. No champion builds, warm-ups, calibration, alternative settings or extra sweeps.
These counts are judge prompt instructions, not a sandbox-enforced execution limit. The judge announces
each benchmark and writes raw JSON evidence. Compilation is excluded from timing. Comparisons use the
SAME numeric efforts, not independently matched wall-time budgets: algorithm changes can change work
per unit of effort, so the judge checks actual research time and validation slowdown separately.

**Hard acceptance floor:** MAX validation gas must improve by at least **0.3% relative to the last
accepted champion**, checked with exact integer arithmetic (`candidate * 1000 <= champion * 997`).
Lower settings must not increase gas *within* the candidate as effort rises. Tests, generality,
complexity and the 5s/20s/120s research timing budgets remain independent vetoes; passing 0.3% alone
does not guarantee acceptance. Below-threshold candidates count toward the three-failure stop; research
set gains alone do not qualify. Final test remains offline. The baseline is already measured for the
active run. A brand-new run needs a one-time trusted champion benchmark at its fixed efforts before
research; it must create `champion-baseline.json` with the schema in `acceptance.ts` (including validation and research
wall times), or baseline lookup will stop rather than quietly rerun it.

Proposal: `{"efforts":[4000,16000,96000],"summary":"..."}`.
Private verdict: `{"accept":true,"feedback":"research-safe aggregates/rationale","evidence":"private checks/measurements","championEfforts":[4000,16000,96000]}`.
The verdict reports submitted efforts as championEfforts for evidence; it cannot retune the saved
champion. In this run, effort settings remain fixed across acceptances and rejections.
Raw measurement evidence is required for acceptance; if it is missing, inconsistent, or has incorrect
coverage the orchestrator pauses. A mistaken judge acceptance below the 0.3% floor is overridden by
the orchestrator and recorded as a rejection.
The judge owns correctness and metric decisions; the runner handles lifecycle, scope checks, commits,
and verdict consumption. Logs retain exact prompts, pi events/usage, and evidence. No fixed spend cap.

## Live progress

Redirect runner output to a **private** log outside the research worktree:

```sh
bun scripts/ssch-research/run.ts run /absolute/private/directory >> /absolute/private/directory/runner.log 2>&1
# In another terminal:
tail -f /absolute/private/directory/runner.log
```

This single stream follows every researcher, judge, and round automatically: timestamped phase
transitions, tool commands/paths, tool completion and duration, assistant updates, retries, and verdicts.
A heartbeat every 30 seconds shows role elapsed time, time since the last event, and active tools (or
waiting for model/API). An active shell command may contain subprocesses; the heartbeat is liveness
information, not proof of useful progress. Raw JSONL and stderr remain in their original locations.
Judge activity can mention private validation paths; do not share this log with the researcher.

Resume using the runner in your main checkout to pick up orchestration fixes; no cherry-pick into the
experiment branch is necessary. The saved champion and next round remain unchanged.

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

Only research data/source snippets may be sent. Before dismissing Jev, the resumed researcher must
actually run a small **oracle experiment** on representative research scheduling decisions (e.g. swap
or operation choice), compare its decisions to a deterministic baseline and deeper search, and save
`jev-pilot.json` linking inputs, typed questions, outputs, latency/usage and results. Reading the skill
or a trivial API ping is not an experiment. Subsequent rounds may cite real negative results rather
than pay for the same experiment again. Jev is a classifier of bounded decisions, **not** a schedule
validator, code generator or runtime compiler dependency. If the service fails, pause rather than
fabricating a negative result. No private validation or archived final-test cases go to Jev.

## Tests

```sh
bun test scripts/ssch-research
(cd scripts/ssch-research && bun install && bun run typecheck)
cargo nextest run -p sir-stack-scheduling -p sir-stack-scheduling-db-bench
```
