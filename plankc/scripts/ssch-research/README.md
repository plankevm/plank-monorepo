# ssch-research

Agents try to improve `sir-stack-scheduling`; a plain script judges them.

```sh
bun scripts/ssch-research/run.ts ~/.local/share/plank-ssch-research/<run>   # start or resume
```

Options (defaults): `--workers 2 --attempts 20 --hours 24 --stall 6 --planners 3
--directions 4 --model claude-bridge/claude-opus-5-5:high`.

**Stop:** Ctrl-C once to stop after the running attempts are judged. Press it again to abort at once.
**Resume:** rerun the same command. Unfinished attempts are discarded.

## How it works

- Attempt 0 judges the current `HEAD` and becomes the first champion.
- Each attempt gets a worktree on branch `ssch-research/<run>/<id>` that starts at the champion, plus a
  read-only copy of the research DB. A fresh agent works there and commits. It may also start from
  or combine any earlier attempt.
- The judge builds, runs `cargo nextest run -p sir-stack-scheduling -p ssch` and benchmarks the
  research set, where an invalid schedule is a failure. If the candidate beats the champion's
  research gas (both at effort 23000), the judge runs the validation set twice: at effort 1000 it
  must finish in under 1s (pass/fail only), and at effort 23000 it must finish within 23s. The
  candidate becomes champion (branch `ssch-research/<run>/champion`) if its effort-23000 validation
  gas is lower at all. The efforts and caps are constants at the top of `run.ts`.
- After `--stall` attempts in a row without a new champion, planners each look at part of the
  champion's per-graph dashboard and part of the archive. Each proposes directions; an LLM drops
  the duplicates, and every remaining direction gets one attempt.
- Run dir: `archive.jsonl` holds one line per attempt or plan and is the whole state.
  `champion-per-graph.tsv` holds the champion's research per-graph results. `prompts/` holds the
  prompts. `logs/` holds every agent session (`<id>.jsonl` for attempts, `plan-<round>-<n>.jsonl`
  for planners). `target-<n>/` are the per-worker cargo target dirs. `wt/` holds attempts in progress.

**Watch:** the script's stdout logs one line per event. Follow an agent live with
`tail -f <run>/logs/<id>.jsonl`, or just its messages with
`tail -f <run>/logs/<id>.jsonl | jq -c 'select(.type == "message") | .message.content'`.
Past attempts: `jq -c '{id, accepted, research: .research.total_gas, validation_low, validation}' <run>/archive.jsonl`.

Data: research `research.sqlite3` and validation `validation.sqlite3` from the
`20260922-125218` split. Agents see only the research data. The judge reports only the
validation total and time.
