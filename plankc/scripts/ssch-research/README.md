# ssch-research

Agents try to improve `sir-stack-scheduling`; a plain script judges them.

```sh
bun scripts/ssch-research/run.ts ~/.local/share/plank-ssch-research/<run>   # start or resume
```

Options (defaults): `--workers 2 --attempts 20 --hours 24 --stall 6 --planners 3
--directions 4 --time-cap 23 --model claude-bridge/claude-opus-5-5:high`.

**Stop:** Ctrl-C once to stop after the running attempts are judged. Press it again to abort at once.
**Resume:** rerun the same command. Unfinished attempts are discarded.

## How it works

- Attempt 0 judges the current `HEAD` and becomes the first champion.
- Each attempt gets a worktree on branch `ssch-research/<run>/<id>` that starts at the champion, plus a
  read-only copy of the research DB. A fresh agent works there and commits. It may also start from
  or combine any earlier attempt.
- The judge builds, runs `cargo nextest run -p sir-stack-scheduling -p ssch` and benchmarks the
  research set, where an invalid schedule is a failure. If the candidate beats the champion's
  research gas, the judge also runs the validation set. The candidate becomes champion (branch
  `ssch-research/<run>/champion`) if its validation gas is lower at all and its run takes under
  `--time-cap` seconds.
- After `--stall` attempts in a row without a new champion, planners each look at part of the
  champion's per-graph dashboard and part of the archive. Each proposes directions; an LLM drops
  the duplicates, and every remaining direction gets one attempt.
- Run dir: `archive.jsonl` holds one line per attempt or plan and is the whole state.
  `champion-per-graph.tsv` holds the champion's research per-graph results. `prompts/` holds the
  prompts. `target-<n>/` are the per-worker cargo target dirs. `wt/` holds attempts in progress.

Data: research `research.sqlite3` and validation `validation.sqlite3` from the
`20260922-125218` split. Agents see only the research data. The judge reports only the
validation total and time.
