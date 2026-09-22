#!/usr/bin/env bun
import { spawn } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, openSync, closeSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
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
    console.log("Stopping after this round. Ctrl-C again aborts the agent; rerun to resume.");
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
  try {
    child = spawn("pi", ["--mode", "json", "--print", "--no-session", "--no-extensions", "--no-prompt-templates",
      "--model", "openai-codex/gpt-6-astra", "--thinking", role === "research" ? "low" : "high",
      "--tools", "read,bash,edit,write", "--", `@${promptPath}`], {
      cwd, detached: true, stdio: ["ignore", output, errors],
      env: { ...process.env, PI_SKIP_VERSION_CHECK: "1" },
    });
    const code = await new Promise<number | null>((done, fail) => {
      child!.once("error", fail);
      child!.once("exit", done);
    });
    if (code !== 0) throw new Error(`Pi stopped (${code}); resume without consuming a round. See ${logPath}`);
    const events = readFileSync(logPath, "utf8").split("\n").filter(Boolean).map(line => JSON.parse(line));
    const messages = events.filter(e => e.type === "message_end" && e.message?.role === "assistant");
    const last = messages.at(-1)?.message;
    if (!last || ["error", "aborted"].includes(last.stopReason)) throw new Error(`Pi did not complete; see ${logPath}`);
  } finally { child = undefined; closeSync(output); closeSync(errors); }
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
    console.log(`Round ${state.round}, phase ${state.phase}, consecutive failures ${state.failures}`);
    if (state.phase === "research") {
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
Goal: best practical gas at MAX effort, then high, then baseline, with a coherent approach viable in a real compiler. Explore algorithms, not just simplifications. Preserve schedule_graph_with_effort's deterministic positive integer effort API and ordinary compiler defaults. Higher effort must not worsen total gas; retain an incumbent where possible. Compiler operation must remain offline.
Build: cargo build --release -p sir-stack-scheduling-db-bench
Find binary directory via cargo metadata --no-deps --format-version 1 (target_directory).
Evaluate: <target_directory>/release/sir-stack-scheduling-db-bench --evaluate ${JSON.stringify(researchDB)} MAX_CANDIDATES
This mode validates schedules and never updates the DB. Measure whole process wall time externally too. Tune THREE efforts to approximate research-set budgets 5s, 20s, 120s. Current champion efforts: ${JSON.stringify(state.championEfforts)}. Test a couple times; leave reasonable headroom. Gas is judged privately, timing here is your measuring stick.
Use cargo nextest run -p sir-stack-scheduling and -p sir-stack-scheduling-db-bench. You may fix mistakes before submission. Final broken proposals count as failed rounds.
Jev: read ${skill}. You may experiment with graph features, swap choices, and learning deterministic offline heuristics. Send only research data/source snippets. Use bun scripts/ssch-research/jev.ts REQUEST.json OUTPUT.json; TYPESAFE_API_KEY is inherited. Never print credentials. Compare against a non-Jev baseline. Save prompts, answers, usage, latency, experiments and whether Jev was helpful/unhelpful/inconclusive (or not used) in ${publicRound}. Keep notes accessible to future rounds. No compiler API dependency on Jev.
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
    }
    if (state.phase === "judge") {
      let proposal: Proposal;
      try {
        proposal = load<Proposal>(proposalPath);
        efforts(proposal.efforts);
        if (typeof proposal.summary !== "string") throw new Error("Missing summary");
      } catch {
        save(verdictPath, { accept: false, feedback: "Rejected: missing or malformed proposal.json", evidence: "Proposal schema check failed", championEfforts: state.championEfforts });
        proposal = { efforts: state.championEfforts, summary: "Malformed proposal" };
      }
      if (!existsSync(verdictPath)) await agent(cwd, "judge", privateRound, `You are the independent stack scheduler judge. Run the checks YOURSELF using shell tools. Do not edit candidate source or commit anything. Candidate ${state.candidate}; champion ${state.champion}. Review git diff ${state.champion} ${state.candidate}.
Research proposal: ${JSON.stringify(proposal)}. Public notes: ${publicRound}.
Private validation DB: ${m.files.validation!.path}. Research DB: ${researchDB}. ALL private logs, failing validation traces, and detailed evidence belong ONLY in ${privateRound}. Do not place validation cases/hashes in public notes or research feedback. Do not inspect final-test data. Do not send validation data to Jev.
Build candidate release bench and run cargo nextest run -p sir-stack-scheduling and -p sir-stack-scheduling-db-bench. Compilation failures, invalid schedules, crashes, or missing coverage mean REJECT. Frozen benchmark/validator/cost-model changes or graph-specific hacks mean REJECT.
Find target_directory using cargo metadata. Benchmark command: <target_directory>/release/sir-stack-scheduling-db-bench --evaluate DATABASE EFFORT. It is read-only and emits aggregate JSON. Measure process wall time as well. Run a couple times at EACH of the three submitted efforts on research AND validation. Do not run concurrent timing benchmarks. Save exact commands/results as private evidence.
Compare against champion using a separate detached worktree under ${privateRound}/champion, with its own target directory; do not checkout/reset the candidate. Champion efforts: ${JSON.stringify(state.championEfforts)}. Calibrate champion efforts on the research set to the same 5s/20s/120s budgets if the saved values are unsuitable. Return the actual championEfforts in your verdict so calibration is retained even when a candidate is rejected. Always compare fairly under the same timing budgets.
Require total validation gas to be non-increasing across candidate effort levels. Research whole-run budgets: 5s baseline, 20s high, 120s max. Exercise judgment around slight overruns/noise. Validation timings need not fit those absolute budgets; reject unusual slowdown relative to champion/workload. Cancel clearly runaway benchmark processes rather than waiting indefinitely.
Prioritize improved MAX validation gas, then high, then baseline. Lower-mode gas regressions are not automatic vetoes; weigh them. Require an actual gas improvement somewhere, not just increased effort outside comparable budgets. Reject negligible gains bought with disproportionate algorithmic complexity. Goal is how well a GENERAL scalable offline compiler scheduler can reasonably do, not simplification for its own sake.
Write ${verdictPath} as {"accept":boolean,"feedback":"research-safe aggregate results and actionable rationale, no private examples or hashes","evidence":"private measurements, exact efforts for both candidate and champion, test outcomes and reasoning","championEfforts":[positiveInteger,positiveInteger,positiveInteger]}. Save detailed evidence files here if needed. You are a judge: do not fix the candidate. Remove your temporary champion worktree when done if practical. If infrastructure makes evaluation impossible, do NOT invent a rejection or verdict; explain the blocker and stop.`);
      const verdict = load<Verdict>(verdictPath);
      efforts(verdict.championEfforts);
      if (typeof verdict.accept !== "boolean" || typeof verdict.feedback !== "string" || typeof verdict.evidence !== "string") throw new Error("Malformed verdict; inspect private notes and resume");
      if (git(state.worktree, "rev-parse", "HEAD") !== state.candidate || git(state.worktree, "diff", "HEAD", "--")) throw new Error("Judge changed candidate; inspect before resuming");
      checkData(m);
      if (sha(researchDB) !== m.files.research!.sha256) throw new Error("Research DB changed");
      save(resolve(publicRound, "feedback.json"), { accept: verdict.accept, feedback: verdict.feedback });
      state.championEfforts = verdict.championEfforts;
      state.accepted = verdict.accept;
      state.phase = "promote";
      save(statePath, state);
    }
    if (state.phase === "promote") {
      if (state.accepted) {
        state.champion = state.candidate!;
        state.championEfforts = load<Proposal>(proposalPath).efforts;
        state.failures = 0;
      } else state.failures++;
      save(resolve(privateRound, "outcome.json"), { ...state });
      console.log(`Round ${state.round}: ${state.accepted ? "accepted" : "rejected"}; champion ${state.champion}`);
      git(state.worktree, "reset", "--hard", state.champion);
      state.round++;
      state.phase = "research";
      delete state.candidate;
      delete state.accepted;
      save(statePath, state);
    }
  }
  console.log(`Stopped. Completed ${state.round - 1} rounds; champion ${state.champion}. Notes: ${notes}`);
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
