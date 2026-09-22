#!/usr/bin/env bun
import { spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, openSync, closeSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { log, progress } from "./progress";
import { baseline, measurements, qualifies, saveBaseline } from "./acceptance";
import { ROOT, activate, prepare, load, save, sha, outside, type Manifest } from "./data";

interface State {
  worktree: string;
  branch: string;
  champion: string;
  round: number;
  failures: number;
  phase: "research" | "judge" | "promote";
  candidate?: string;
  accepted?: boolean;
  championEfforts: number[];
}
interface Proposal { efforts: number[]; summary: string }
interface Verdict { accept: boolean; feedback: string; evidence: string; championEfforts: number[] }
let stopping = false;
let child: ReturnType<typeof spawn> | undefined;
process.on("SIGINT", () => {
  if (stopping) {
    if (child?.pid) process.kill(-child.pid, "SIGTERM");
    process.exitCode = 130;
  } else {
    stopping = true;
    log("Stopping after this round. Ctrl-C again aborts the agent; rerun to resume.");
  }
});

function git(cwd: string, ...args: string[]) {
  const p = Bun.spawnSync(["git", ...args], { cwd, stdout: "pipe", stderr: "pipe" });
  if (p.exitCode) throw new Error(p.stderr.toString());
  return p.stdout.toString().trim();
}
export function efforts(value: unknown): asserts value is number[] {
  if (!Array.isArray(value) || value.length !== 3 || value.some(n => !Number.isSafeInteger(n) || n <= 0) || value[0] > value[1] || value[1] > value[2]) {
    throw new Error("Expected three nondecreasing positive integer efforts");
  }
}
export function allowed(path: string) {
  return path.startsWith("plankc/sir/crates/stack-scheduling/src/") &&
    !/^plankc\/sir\/crates\/stack-scheduling\/src\/(validation\.rs|display\.rs|stack\.rs|stack_ops\.rs|op_graph\/)/.test(path);
}

async function agent(cwd: string, role: "research" | "judge", directory: string, prompt: string) {
  mkdirSync(directory, { recursive: true });
  const promptPath = resolve(directory, `${role}-prompt.md`);
  writeFileSync(promptPath, prompt);
  const logPath = resolve(directory, `${role}-${Date.now()}.jsonl`);
  const output = openSync(logPath, "wx", 0o600);
  const errors = openSync(`${logPath}.stderr`, "wx", 0o600);
  const monitor = progress(role);
  const heartbeat = setInterval(() => monitor.heartbeat(), 30_000);
  let streamError: Error | undefined;
  log(`${role} starting; events: ${logPath}`);
  try {
    child = spawn("pi", ["--mode", "json", "--print", "--no-session", "--no-extensions", "--no-prompt-templates",
      "--model", role === "research" ? "openai-codex/gpt-6-sol" : "openai-codex/gpt-6-astra", "--thinking", "high",
      "--tools", "read,bash,edit,write", "--", `@${promptPath}`], {
      cwd, detached: true, stdio: ["ignore", "pipe", errors],
      env: { ...process.env, PI_SKIP_VERSION_CHECK: "1" },
    });
    child.stdout!.on("data", (chunk: Buffer) => {
      try {
        writeFileSync(output, chunk);
        if (!streamError) monitor.push(chunk);
      } catch (error) { streamError = error as Error; }
    });
    const code = await new Promise<number | null>((done, fail) => {
      child!.once("error", fail);
      child!.once("close", done);
    });
    if (code !== 0) throw new Error(`Pi stopped (${code}); resume without consuming a round. See ${logPath}`);
    if (streamError) throw streamError;
    const last = monitor.finish();
    if (!last || ["error", "aborted"].includes(last.stopReason ?? "")) throw new Error(`Pi did not complete; see ${logPath}`);
    log(`${role} completed`);
  } finally { clearInterval(heartbeat); child = undefined; closeSync(output); closeSync(errors); }
}

function checkData(m: Manifest) {
  if (!m.acknowledged) throw new Error("Complete the archive handoff and activate first");
  for (const name of ["research", "validation"]) {
    if (sha(m.files[name]!.path) !== m.files[name]!.sha256) throw new Error(`${name} DB was modified`);
  }
  const leftovers = m.removeBeforeResearch.filter(p => p !== m.source && existsSync(p));
  if (leftovers.length) throw new Error(`Full-data copies remain: ${leftovers.join(", ")}`);
  if (sha(m.source) !== m.files.research!.sha256) throw new Error("Canonical DB is not the research partition");
}

async function run(directory: string) {
  const m = load<Manifest>(resolve(directory, "manifest.json"));
  checkData(m);
  const statePath = resolve(directory, "state.json");
  let state: State;
  const repo = git(ROOT, "rev-parse", "--show-toplevel");
  if (existsSync(statePath)) state = load<State>(statePath);
  else {
    if (git(repo, "status", "--porcelain", "--", "plankc/scripts/ssch-research", "plankc/devtools/stack-scheduling-db-bench", "plankc/sir/crates/stack-scheduling")) {
      throw new Error("Commit the research runner and benchmark/scheduler changes before starting");
    }
    const worktree = resolve(directory, "experiment");
    state = { worktree, branch: `ssch-research-${Date.now()}`, champion: git(repo, "rev-parse", "HEAD"), round: 1, failures: 0, phase: "research", championEfforts: [4000, 16000, 96000] };
    save(statePath, state);
  }
  if (!outside(ROOT, state.worktree) || !state.worktree.startsWith(`${directory}/`)) throw new Error("Unexpected worktree path in state");
  if (!existsSync(resolve(state.worktree, ".git"))) {
    const branchExists = git(repo, "branch", "--list", state.branch).length > 0;
    git(repo, "worktree", "add", ...(branchExists ? [state.worktree, state.branch] : ["-b", state.branch, state.worktree, state.champion]));
  }
  const cwd = resolve(state.worktree, "plankc");
  const notes = resolve(cwd, "tmp/ssch-research");
  mkdirSync(notes, { recursive: true });
  const researchDB = resolve(cwd, "corpus/stack-scheduling-db/canonical-blocks.sqlite3");
  mkdirSync(dirname(researchDB), { recursive: true });
  if (!existsSync(researchDB)) copyFileSync(m.files.research!.path, researchDB);
  if (sha(researchDB) !== m.files.research!.sha256) throw new Error("Worktree research DB changed");
  const skill = resolve(notes, "typesafe-skill.md");
  if (!existsSync(skill)) copyFileSync(resolve(import.meta.dir, "typesafe-skill.md"), skill);
  console.log(`Experiment: ${cwd}\nChampion: ${state.champion}\nPrivate state: ${statePath}`);
  while (!stopping && state.round <= 20 && state.failures < 3) {
    checkData(m);
    const publicRound = resolve(notes, `round-${state.round}`);
    const privateRound = resolve(directory, `round-${state.round}`);
    mkdirSync(publicRound, { recursive: true });
    mkdirSync(privateRound, { recursive: true });
    const proposalPath = resolve(publicRound, "proposal.json");
    const verdictPath = resolve(privateRound, "verdict.json");
    log(`Round ${state.round}, phase ${state.phase}, consecutive failures ${state.failures}`);
    if (state.phase === "research") {
      const champion = baseline(directory, m, state.champion, state.championEfforts, state.round - 1);
      const marker = resolve(privateRound, "started");
      if (!existsSync(marker)) {
        git(state.worktree, "reset", "--hard", state.champion);
        const untracked = git(state.worktree, "ls-files", "--others", "--exclude-standard").split("\n").filter(Boolean);
        for (const p of untracked.filter(allowed)) rmSync(resolve(state.worktree, p), { recursive: true });
        writeFileSync(marker, state.champion);
      }
      await agent(cwd, "research", publicRound, `You are researching GENERAL scalable stack scheduling algorithms, not memorizing graphs.
This is round ${state.round}/20. Champion commit: ${state.champion}. Previous notes: ${notes}.
Use ONLY the research DB at ${researchDB}. Do not search for validation/final-test cases, backups, original source corpora, private judge notes, or other sessions. Avoid poisoning your context with those cases. This is cooperative separation, not sandboxing.
You may modify scheduler implementation/tests under sir/crates/stack-scheduling/src, except validation.rs, display.rs, stack.rs, stack_ops.rs, and op_graph/. Do not modify the evaluator, cost model, validators, datasets, manifests, dependencies, compiler interfaces outside this crate, or orchestration. Do not commit, change branches, or spawn agents.
Goal: a coherent GENERAL algorithm with much better MAX-effort gas. The current champion is still ${champion.validationGas[2]} gas on private validation versus 33891 best-known (the best-known is not a proof of optimality). Research can see only that aggregate; do NOT seek private examples. Recent incremental passes yielded tiny gains or no validation gains. Break out of local beam-width tweaks: you MAY replace or rewrite the scheduler from scratch within the allowed scheduler files, change internal search architecture and algorithms, add tests, and discard obsolete passes; do not alter frozen graph/validator/cost/evaluator code or the public schedule_graph_with_effort API. Keep compiler default practical and online behavior deterministic/offline. Higher effort must not worsen gas. An accepted candidate must reduce MAX validation total by AT LEAST 0.3% from the last accepted champion: currently at least ${Math.ceil(champion.validationGas[2]! * 0.003)} gas saved at the fixed efforts. Improvement only on the research set, or below this bar, is a rejected round. You cannot see validation cases. Do not overfit the aggregate feedback. Think outside the box: spend time on a genuinely different algorithm and research-set experiments instead of paying for another minor beam pass.
Specific idea to implement and measure: after an initial schedule with locally greedy per-operation swaps, optimize bounded windows of a basic block. Pin operations outside the window and some operations inside, but leave a small subset free. Explore BOTH their legal operation orders AND actual stack operations (swaps/dups/spills etc.), allowing temporarily costly swaps that enable lower final gas. Use the current valid window schedule's actual cost as an upper bound for a deeper branch-and-bound/DFS search; prune only with an admissible lower bound, accounting for window entry/exit stack state and effects. Repair/pin the improved window and retain the valid full-block incumbent. Compare against the champion on research; document whether it helps and why if not. This is a hypothesis, not a mandate to force a bad design. Explore other structural approaches too.
Build: cargo build --release -p sir-stack-scheduling-db-bench
Find binary directory via cargo metadata --no-deps --format-version 1 (target_directory).
Evaluate: <target_directory>/release/sir-stack-scheduling-db-bench --evaluate ${JSON.stringify(researchDB)} MAX_CANDIDATES
This mode validates schedules and never updates the DB. Measure whole process wall time externally too. Efforts are FIXED for the rest of this run at ${JSON.stringify(state.championEfforts)}; submit EXACTLY these three values. For speed judge against the 5s/20s/120s research-set budgets with some headroom. You may tune algorithm behavior within these budgets but not raise the submitted effort settings, since the cached champion baseline exists only at these efforts. Compare locally against champion on the research set.
Use cargo nextest run -p sir-stack-scheduling and -p sir-stack-scheduling-db-bench. You may fix mistakes before submission. Final broken proposals count as failed rounds.
Jev is an IMPORTANT research experiment, not merely an optional suggestion: rounds 1–10 never called it. Before dismissing it, run a bounded, actual oracle-style pilot on RESEARCH graphs: ask narrow typed judgments (choice of next swap/operation or rank candidate moves) at selected branch points, compare its choices/costs with a deterministic non-Jev baseline and deeper search, and use the observed outcomes to design an offline deterministic heuristic. Read ${skill} and live docs for the API and question design. Use bun scripts/ssch-research/jev.ts REQUEST.json OUTPUT.json; TYPESAFE_API_KEY is inherited. Never print credentials or send private validation/final-test data. Keep the pilot small (e.g. 20–50 diverse research states) and record input sampling method, prompts, responses, usage, latency, comparison metric, and failure analysis in ${publicRound}. Jev is NOT a reasoning/code-generation model or correctness oracle; do not feed it entire graph search trees or require it at compiler runtime. ${state.round === 11 ? "This run has NOT had a documented pilot yet: in this resumed round, actually invoke Jev and save a jev-pilot.json linking the requests, responses and measured non-Jev comparator; a mere skill read or API ping does not count." : `Cite a genuine earlier pilot such as ${resolve(notes, "round-11/jev-pilot.json")} if it exists; if none exists, run one now.`} In later rounds you may build on a genuine prior pilot instead of repeating identical calls. If the API is unavailable, preserve work and pause rather than claiming it was unhelpful. If Jev fails to help, document evidence; do not fake a win. Keep notes accessible to future rounds.
Write ${proposalPath} as {"efforts":[positiveInteger,positiveInteger,positiveInteger],"summary":"hypothesis, changes, measurements, tests, Jev findings"}. Efforts must be nondecreasing. Keep detailed notes in this round directory. Finish with one coherent candidate. If resuming, inspect existing edits/notes rather than discarding work.`);
      const changed = git(state.worktree, "diff", "--name-only", "HEAD").split("\n").filter(Boolean);
      const untracked = git(state.worktree, "ls-files", "--others", "--exclude-standard").split("\n").filter(Boolean);
      if ([...changed, ...untracked].some(p => !allowed(p))) throw new Error("Out-of-scope changes; inspect worktree before resuming");
      git(state.worktree, "add", "--", "plankc/sir/crates/stack-scheduling/src");
      git(state.worktree, "commit", "--allow-empty", "-m", `ssch research: round ${state.round} candidate`);
      state.candidate = git(state.worktree, "rev-parse", "HEAD");
      git(state.worktree, "update-ref", `refs/ssch-research/${state.worktree.split("/").slice(-2, -1)[0]}/round-${state.round}`, state.candidate);
      state.phase = "judge";
      save(statePath, state);
      log(`Round ${state.round}: research → judge; candidate ${state.candidate}`);
    }
    if (state.phase === "judge") {
      const champion = baseline(directory, m, state.champion, state.championEfforts, state.round - 1);
      let proposal: Proposal;
      try {
        proposal = load<Proposal>(proposalPath);
        efforts(proposal.efforts);
        if (JSON.stringify(proposal.efforts) !== JSON.stringify(state.championEfforts)) throw new Error("Efforts must match cached champion");
        if (typeof proposal.summary !== "string") throw new Error("Missing summary");
      } catch {
        save(verdictPath, { accept: false, feedback: "Rejected: missing or malformed proposal.json", evidence: "Proposal schema check failed", championEfforts: state.championEfforts });
        proposal = { efforts: state.championEfforts, summary: "Malformed proposal" };
      }
      if (!existsSync(verdictPath)) await agent(cwd, "judge", privateRound, `You are the independent stack scheduler judge. Run the checks YOURSELF using shell tools. Do not edit candidate source or commit anything. Candidate ${state.candidate}; champion ${state.champion}. Review git diff ${state.champion} ${state.candidate}.
Research proposal: ${JSON.stringify(proposal)}. Public notes: ${publicRound}. Cached champion commit ${champion.commit}, research/validation DB SHA256 ${champion.researchSha256}/${champion.validationSha256}. At fixed efforts ${JSON.stringify(champion.efforts)}, last accepted champion validation gas ${JSON.stringify(champion.validationGas)} and research wall seconds (slowest of two runs) ${JSON.stringify(champion.researchWallSeconds)} and validation wall seconds ${JSON.stringify(champion.validationWallSeconds)}. The cached raw benchmark measurements are in the private previous accepted round, NOT in the research worktree. Do not show or copy private paths or per-case evidence to research.
Private validation DB: ${m.files.validation!.path}. Research DB: ${researchDB}. ALL private logs, failing validation traces, and detailed evidence belong ONLY in ${privateRound}. Do not place validation cases/hashes in public notes or research feedback. Do not inspect final-test data. Do not send validation data to Jev.
Inspect this round's Jev pilot at ${publicRound}/jev-pilot.json (or the earlier real pilot referenced there). Before building or benchmarking, check whether a genuine prior Jev pilot exists. If none exists, this round MUST actually trial Jev on training decisions and compare with a deterministic baseline; reading docs or a trivial API ping does not count. If absent, REJECT as incomplete research without spending time benchmarking (unless there is an API outage: stop without writing a verdict). Treat an actual failed pilot as useful negative evidence, not a requirement to deploy Jev. Build candidate release bench and run cargo nextest run -p sir-stack-scheduling and -p sir-stack-scheduling-db-bench. Compilation failures, invalid schedules, crashes, or missing coverage mean REJECT. Frozen benchmark/validator/cost-model changes or graph-specific hacks mean REJECT.
Find target_directory using cargo metadata. Benchmark command: <target_directory>/release/sir-stack-scheduling-db-bench --evaluate DATABASE EFFORT. It is read-only and emits aggregate JSON. Measure process wall time as well. Run exactly TWO repetitions at EACH of the three fixed efforts on research AND validation for the CANDIDATE ONLY: 12 planned benchmark executions (2 datasets × 3 efforts × 2 repetitions). A THIRD repetition at an existing candidate setting is allowed ONLY to resolve timing noise. No warm-ups, calibration, alternative efforts, exploratory timing probes, or extra sweeps. Do NOT build, run, or benchmark the old champion. Use only the cached results above and check that the effort settings and DB checksums match; otherwise stop and report the mismatch rather than measuring a new baseline. Save raw results at ${privateRound}/candidate-{research|validation}-EFFORT-REPETITION.measurement.json containing {"returncode":0,"wall_seconds":number,"result":{"graphs":number,"effort":number,"total_gas":number,...}} for EACH run. Data must come from the actual evaluator output; no estimates. Do not run concurrent timing benchmarks. Before each benchmark announce implementation, dataset, effort, and repetition; invoke measurements separately so tool progress identifies the active run. Save exact commands/results as private evidence.
Compare the candidate against cached champion at the SAME submitted numeric efforts ${JSON.stringify(champion.efforts)}; numerical effort may represent different work after algorithm changes, so also compare research wall time against 5s/20s/120s and the cached wall timings. Do not retune the champion. Return those fixed efforts as championEfforts in the verdict.
Require total validation gas to be non-increasing across candidate effort levels. Research whole-run budgets: 5s baseline, 20s high, 120s max. Exercise judgment around slight overruns/noise. Validation timings need not fit those absolute budgets; reject unusual slowdown relative to champion/workload. Cancel clearly runaway benchmark processes rather than waiting indefinitely.
HARD PROMOTION FLOOR: accept ONLY if MAX validation total gas falls by AT LEAST 0.3% relative to the cached last accepted champion ${champion.validationGas[2]}: candidate_max * 1000 <= champion_max * 997. At this champion the MAX candidate must be <= ${Math.floor(champion.validationGas[2]! * 997 / 1000)} gas (save >= ${Math.ceil(champion.validationGas[2]! * 0.003)}). No exceptions for promising research gas, speed, or cleanliness; below threshold REJECT, even if every test passes. A result meeting the threshold is not automatically accepted: correctness, generality, research time budget, non-increasing gas with effort and reasonable complexity still matter. Baseline/high gas regressions are weighed, not automatic vetoes. If the candidate exceeds its research timing budget materially, reject, even if its numeric-effort score is better. Goal is how well a GENERAL scalable offline compiler scheduler can reasonably do, not simplification for its own sake.
Write ${verdictPath} as {"accept":boolean,"feedback":"research-safe aggregate results and actionable rationale, no private examples or hashes","evidence":"private measurements, exact efforts for both candidate and champion, test outcomes and reasoning","championEfforts":[positiveInteger,positiveInteger,positiveInteger]}. Save detailed evidence files here if needed. You are a judge: do not fix the candidate. Remove your temporary champion worktree when done if practical. If infrastructure makes evaluation impossible, do NOT invent a rejection or verdict; explain the blocker and stop.`);
      const verdict = load<Verdict>(verdictPath);
      efforts(verdict.championEfforts);
      if (typeof verdict.accept !== "boolean" || typeof verdict.feedback !== "string" || typeof verdict.evidence !== "string") throw new Error("Malformed verdict; inspect private notes and resume");
      if (verdict.accept) {
        const runs = measurements(privateRound, proposal.efforts, m.files.validation!.count!);
        measurements(privateRound, proposal.efforts, m.files.research!.count!, "candidate", "research");
        const gas = runs.map(pair => pair[0]!.result.total_gas);
        if (gas.some((value, i) => i > 0 && value > gas[i - 1]!)) throw new Error("Judge accepted non-monotone validation gas; inspect private evidence");
        if (!qualifies(champion.validationGas[2]!, gas[2]!)) {
          verdict.accept = false;
          verdict.feedback = `Rejected by 0.3% MAX validation floor: champion ${champion.validationGas[2]}, candidate ${gas[2]}. ${verdict.feedback}`;
          save(verdictPath, verdict);
        }
      }
      if (git(state.worktree, "rev-parse", "HEAD") !== state.candidate || git(state.worktree, "diff", "HEAD", "--")) throw new Error("Judge changed candidate; inspect before resuming");
      checkData(m);
      if (sha(researchDB) !== m.files.research!.sha256) throw new Error("Research DB changed");
      save(resolve(publicRound, "feedback.json"), { accept: verdict.accept, feedback: verdict.feedback });
      state.accepted = verdict.accept;
      state.phase = "promote";
      save(statePath, state);
      log(`Round ${state.round}: judge → promote; ${verdict.accept ? "accepted" : "rejected"}. ${verdict.feedback}`);
    }
    if (state.phase === "promote") {
      if (state.accepted) {
        state.champion = state.candidate!;
        state.championEfforts = load<Proposal>(proposalPath).efforts;
        saveBaseline(directory, m, state.champion, state.championEfforts, state.round);
        state.failures = 0;
      } else state.failures++;
      save(resolve(privateRound, "outcome.json"), { ...state });
      log(`Round ${state.round}: ${state.accepted ? "accepted" : "rejected"}; champion ${state.champion}`);
      git(state.worktree, "reset", "--hard", state.champion);
      state.round++;
      state.phase = "research";
      delete state.candidate;
      delete state.accepted;
      save(statePath, state);
    }
  }
  log(`Stopped. Completed ${state.round - 1} rounds; champion ${state.champion}. Notes: ${notes}`);
}

if (import.meta.main) {
  const [command, rawDirectory, flag] = process.argv.slice(2);
  if (!rawDirectory || !["prepare", "activate", "run"].includes(command ?? "")) {
    console.log("Usage: bun scripts/ssch-research/run.ts prepare|activate|run PRIVATE_DIRECTORY [--writers-stopped|--archive-confirmed]");
    process.exit(1);
  }
  const directory = resolve(rawDirectory);
  if (!outside(ROOT, directory)) throw new Error("Use a private directory outside plankc");
  if (command === "prepare") {
    if (flag !== "--writers-stopped") throw new Error("Stop all corpus writers and keep them stopped through activation; confirm with --writers-stopped");
    prepare(directory);
  }
  else if (command === "activate") {
    if (flag !== "--archive-confirmed") throw new Error("Confirm cloud archive verification and local removal with --archive-confirmed");
    activate(directory);
  } else {
    const lock = resolve(directory, ".runner-lock");
    mkdirSync(lock);
    writeFileSync(resolve(lock, "pid"), String(process.pid));
    try { await run(directory); } finally { rmSync(lock, { recursive: true }); }
  }
}
