#!/usr/bin/env bun
import { spawn, execFileSync, type ChildProcess } from "node:child_process";
import { appendFileSync, chmodSync, copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, resolve } from "node:path";
import { parseArgs } from "node:util";

const { values: opt, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    workers: { type: "string", default: "2" },
    attempts: { type: "string", default: "20" },
    hours: { type: "string", default: "24" },
    minutes: { type: "string", default: "60" },
    stall: { type: "string", default: "6" },
    planners: { type: "string", default: "3" },
    directions: { type: "string", default: "4" },
    model: { type: "string", default: "claude-bridge/claude-opus-5-5:high" },
  },
});
if (positionals.length !== 1) {
  console.log("Usage: bun scripts/ssch-research/run.ts RUN_DIR [--workers 2] [--attempts 20] [--hours 24] [--minutes 60] [--stall 6] [--planners 3] [--directions 4] [--model M]");
  process.exit(1);
}
const RUN = resolve(positionals[0]!);
const WORKERS = Number(opt.workers), MAX_ATTEMPTS = Number(opt.attempts), STALL = Number(opt.stall);
const PLANNERS = Number(opt.planners), DIRECTIONS = Number(opt.directions);
// Validation runs twice: low effort is a pass/fail speed gate, high effort is what's compared on gas.
const LOW_EFFORT = 1000, LOW_TIME_CAP = 1, HIGH_EFFORT = 23000, HIGH_TIME_CAP = 23;
const DEADLINE = Date.now() + Number(opt.hours) * 3_600_000;
// Each researcher's time budget; time while the run is stopped or the machine sleeps doesn't count.
const AGENT_MINUTES = Number(opt.minutes);

const DATA = `${homedir()}/.local/share/plank-ssch-research/20260922-125218`;
const RESEARCH_DB = `${DATA}/research.sqlite3`;
const VALIDATION_DB = `${DATA}/validation.sqlite3`;
const PLANKC = resolve(import.meta.dir, "../..");
const TOP = git(PLANKC, "rev-parse", "--show-toplevel");
const ARCHIVE = `${RUN}/archive.jsonl`;
const BRANCH = `ssch-research/${basename(RUN)}`;
const PER_GRAPH = `${RUN}/champion-per-graph.tsv`;
const PIDS = `${RUN}/pids`;
// Only the auth extension: the user's other extensions (notifications, handoffs) must not fire from agents.
const PI = ["pi", "-p", "--no-extensions", "-e", `${homedir()}/.pi/agent/extensions/claude-auth.ts`, "-e", `${import.meta.dir}/time-left.ts`, "--model", opt.model!];

interface Bench { total_gas: number; best_known_gas: number; graphs_at_best_known: number; scheduler_seconds: number; wall_seconds: number }
interface Tier { total_gas: number; wall_seconds: number }
interface Attempt {
  kind: "attempt"; id: number; commit: string; accepted: boolean; direction?: string;
  research?: Bench; validation_low?: Tier; validation?: Tier; error?: string; loc: number; notes: string;
}
interface Plan { kind: "plan"; directions: string[] }
type Entry = Attempt | Plan;
// An attempt in progress, persisted in wt/<id>.json so a killed run resumes it. The agent is done once notes or timedOut is set.
interface Live { id: number; slot: number; start: string; direction?: string; deadline: number; updated: number; notes?: string; timedOut?: boolean }

let stopping = false;
const children = new Map<ChildProcess, string>();
process.on("SIGINT", () => {
  if (stopping) {
    for (const child of children) killTree(child.pid!);
    console.log("\nAborted. Rerun the same command to resume.");
    process.exit(130);
  }
  stopping = true;
  console.log("\nStopping after the current attempts finish. Ctrl-C again aborts now.");
});

function git(cwd: string, ...args: string[]) {
  return execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

// Children get their own process group so the terminal's Ctrl-C reaches only this script.
function sh(cmd: string[], cwd: string, env: Record<string, string> = {}, timeoutSeconds?: number, onSpawn?: (child: ChildProcess) => void) {
  return new Promise<{ code: number | null; out: string; err: string; seconds: number }>(done => {
    const start = performance.now();
    const child = spawn(cmd[0]!, cmd.slice(1), { cwd, detached: true, env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
    children.set(child, startTime(child.pid!));
    savePids();
    onSpawn?.(child);
    let out = "", err = "";
    child.stdout!.on("data", chunk => (out += chunk));
    child.stderr!.on("data", chunk => (err += chunk));
    const timer = timeoutSeconds ? setTimeout(() => killTree(child.pid!), timeoutSeconds * 1000) : undefined;
    child.on("close", code => {
      clearTimeout(timer);
      children.delete(child);
      savePids();
      done({ code, out, err, seconds: (performance.now() - start) / 1000 });
    });
  });
}

// pi's bash tool gives each command its own process group, so killing pi's group would leave them running.
function killTree(pid: number) {
  const procs = execFileSync("ps", ["-A", "-o", "pid=,ppid="], { encoding: "utf8" }).trim().split("\n").map(l => l.trim().split(/\s+/).map(Number));
  const tree = [pid];
  for (let i = 0; i < tree.length; i++) for (const [p, parent] of procs) if (parent === tree[i]) tree.push(p!);
  for (const p of tree) try { process.kill(p, "SIGKILL"); } catch {}
}

function startTime(pid: number) {
  try { return execFileSync("ps", ["-o", "lstart=", "-p", String(pid)], { encoding: "utf8" }).trim(); } catch { return ""; }
}

// A killed run leaves its detached children running; the next run kills them before starting its own.
// The start time guards against killing an unrelated process that reused the pid after a reboot.
const savePids = () => writeFileSync(PIDS, [...children].map(([c, start]) => `${c.pid}\t${start}`).join("\n"));
function killOrphans() {
  for (const line of existsSync(PIDS) ? readFileSync(PIDS, "utf8").split("\n").filter(Boolean) : []) {
    const [pid, start] = line.split("\t");
    if (start && startTime(Number(pid)) === start) killTree(Number(pid));
  }
}

// A reused id (after a discarded attempt or interrupted plan) must start a new session, not continue the old one.
function session(name: string) {
  const file = `${RUN}/logs/${name}.jsonl`;
  rmSync(file, { force: true });
  return ["--session", file];
}

const log = (message: string) => console.log(`[${new Date().toLocaleTimeString()}] ${message}`);
const entries = (): Entry[] => existsSync(ARCHIVE) ? readFileSync(ARCHIVE, "utf8").split("\n").filter(Boolean).map(line => JSON.parse(line)) : [];
const attempts = () => entries().filter((e): e is Attempt => e.kind === "attempt");
const champion = () => attempts().findLast(a => a.accepted)!;
const record = (entry: Entry) => appendFileSync(ARCHIVE, JSON.stringify(entry) + "\n");

function stalled() {
  const all = entries();
  const since = all.findLastIndex(e => e.kind === "plan" || e.accepted);
  return all.slice(since + 1).length;
}

function pendingDirections() {
  const all = entries();
  const plan = all.findLastIndex(e => e.kind === "plan");
  if (plan < 0) return [];
  const used = new Set(all.slice(plan).flatMap(e => e.kind === "attempt" && e.direction ? [e.direction] : []));
  return (all[plan] as Plan).directions.filter(d => !used.has(d));
}

function loc(cwd: string) {
  return git(cwd, "ls-files", "sir/crates/stack-scheduling").split("\n").filter(f => f.endsWith(".rs"))
    .reduce((sum, f) => sum + readFileSync(resolve(cwd, f), "utf8").split("\n").length, 0);
}

async function bench(cwd: string, target: string, db: string, effort: number, perGraph?: string): Promise<Bench | string> {
  const args = [`${target}/release/ssch`, "bench", "--json", "-e", String(effort), "-d", db, ...(perGraph ? ["--per-graph", perGraph] : [])];
  const r = await sh(args, cwd, {}, 600);
  if (r.code !== 0) return `bench failed: ${(r.err || r.out).slice(-2000)}`;
  return { ...JSON.parse(r.out), wall_seconds: r.seconds };
}

async function judge(entry: Attempt, cwd: string, target: string, isBaseline: boolean) {
  const env = { CARGO_TARGET_DIR: target };
  for (const cmd of [["cargo", "build", "--release", "-p", "ssch"], ["cargo", "nextest", "run", "-p", "sir-stack-scheduling", "-p", "ssch"]]) {
    const r = await sh(cmd, cwd, env, 3600);
    if (r.code !== 0) return void (entry.error = `${cmd.slice(0, 2).join(" ")} failed:\n${(r.err + r.out).slice(-2000)}`);
  }
  const perGraph = `${cwd}/tmp/per-graph.tsv`;
  mkdirSync(`${cwd}/tmp`, { recursive: true });
  const research = await bench(cwd, target, RESEARCH_DB, HIGH_EFFORT, perGraph);
  if (typeof research === "string") return void (entry.error = research);
  entry.research = research;
  const best = isBaseline ? undefined : champion();
  if (best && research.total_gas >= best.research!.total_gas) return;
  const low = await bench(cwd, target, VALIDATION_DB, LOW_EFFORT);
  if (typeof low === "string") return void (entry.error = "validation run failed (details withheld)");
  entry.validation_low = { total_gas: low.total_gas, wall_seconds: low.wall_seconds };
  if (!isBaseline && low.wall_seconds >= LOW_TIME_CAP) return;
  const high = await bench(cwd, target, VALIDATION_DB, HIGH_EFFORT);
  if (typeof high === "string") return void (entry.error = "validation run failed (details withheld)");
  entry.validation = { total_gas: high.total_gas, wall_seconds: high.wall_seconds };
  entry.accepted = isBaseline || (high.total_gas < best!.validation!.total_gas && high.wall_seconds <= HIGH_TIME_CAP);
  if (entry.accepted) {
    copyFileSync(perGraph, PER_GRAPH);
    git(TOP, "update-ref", `refs/heads/${BRANCH}/champion`, entry.commit);
  }
}

function researcherPrompt(id: number, cwd: string, start: string, direction?: string) {
  return `You are improving the stack scheduler of the Plank compiler: crate \`sir-stack-scheduling\` (sir/crates/stack-scheduling), which turns a basic block's operation graph into stack operations (dupN, swapN, pop, ops, spills) for the EVM.

Goal: lower total gas on graphs you cannot see, within the time limits below, with an algorithm a human can understand and reimplement from sir/crates/stack-scheduling/ALGORITHM.md.

Worktree: ${cwd}, branch ${BRANCH}/${id}, starting at the current champion ${start}.
Archive of all previous attempts, failures included, each a git commit with notes: ${ARCHIVE}. The champion is the last entry with "accepted": true. You may start from any archived commit (git reset --hard <commit>) or combine several (cherry-pick, merge).${direction ? `\n\nDirection to explore, proposed by a planner: ${direction}` : ""}

Research data: corpus/stack-scheduling-db/canonical-blocks.sqlite3, 6,992 graphs with the best-known schedule for each (read-only). Looking at individual graphs is expected.
Tools (cargo build --release -p ssch; binary at $CARGO_TARGET_DIR/release/ssch):
- ssch bench [--json] [--worst N] [--per-graph out.tsv]: schedules and validates every graph, compares with best known.
- ssch inspect <hash> | --random: shows a graph and replays its best-known schedule.
Effort is passed with ssch bench -e N. On the hidden validation set (1,498 graphs), effort ${LOW_EFFORT} must finish in under ${LOW_TIME_CAP}s and effort ${HIGH_EFFORT} in at most ${HIGH_TIME_CAP}s (aim for effort ≈ milliseconds of wall time); gas is compared at effort ${HIGH_EFFORT}. You may rewrite or replace anything in sir-stack-scheduling.

Rules: no per-graph constants or memorized schedules; schedules must be valid; don't look for validation or test data; never pass --save or write to any database; don't write to the archive, the judge records your attempt.

Judging: build, cargo nextest run -p sir-stack-scheduling -p ssch, research bench at effort ${HIGH_EFFORT}, and, if research gas beats the champion, total gas and time on the hidden validation set at both efforts. Any validation gain within the time limits makes you champion.

Time: you have ${AGENT_MINUTES} minutes for this attempt. If you haven't committed and replied by then, you are stopped and the attempt fails without being judged. Call the time_left tool to see how many seconds remain, and leave yourself time to commit and write your reply. Always give bash commands a timeout (the bash tool's timeout parameter), so a slow build, bench or debug run can't silently use up your time.

Deliverable: commit your work on this branch with ALGORITHM.md up to date. Your final reply goes into the archive for future researchers: first line \`parent: <commit>\`, then what you tried, the numbers, and what you learned. Plain and concise.`;
}

const stateFile = (id: number) => `${RUN}/wt/${id}.json`;
const save = (live: Live) => writeFileSync(stateFile(live.id), JSON.stringify(live));
const paths = (live: Live) => ({ wt: `${RUN}/wt/${live.id}`, cwd: `${RUN}/wt/${live.id}/plankc`, target: `${RUN}/target-${live.slot}` });

function prepare(id: number, slot: number, direction?: string): Live {
  const start = id === 0 ? git(TOP, "rev-parse", "HEAD") : champion().commit;
  const live: Live = { id, slot, start, direction, deadline: Date.now() + AGENT_MINUTES * 60_000, updated: Date.now() };
  const { wt, cwd } = paths(live);
  git(TOP, "worktree", "add", "-f", "-B", `${BRANCH}/${id}`, wt, start);
  const db = `${cwd}/corpus/stack-scheduling-db/canonical-blocks.sqlite3`;
  mkdirSync(resolve(db, ".."), { recursive: true });
  copyFileSync(RESEARCH_DB, db);
  chmodSync(db, 0o444);
  if (id === 0) live.notes = "baseline";
  else writeFileSync(`${RUN}/prompts/${id}.md`, researcherPrompt(id, cwd, start, direction));
  save(live);
  return live;
}

async function research(live: Live) {
  const { cwd, target } = paths(live);
  const file = `${RUN}/logs/${live.id}.jsonl`;
  const resumed = existsSync(file) && statSync(file).size > 0;
  // The run was stopped since the last update; that time isn't the agent's.
  live.deadline += Date.now() - live.updated;
  live.updated = Date.now();
  save(live);
  const left = () => Math.round((live.deadline - Date.now()) / 1000);
  if (left() <= 0) return void save(Object.assign(live, { timedOut: true }));
  log(`attempt ${live.id} ${resumed ? `resuming, ${left()}s left` : "researching"}${live.direction ? `: ${live.direction}` : ""}`);
  const message = resumed
    ? ["--session", file, `You were interrupted and have now been resumed. ${left()} seconds left. Continue where you left off; the deliverable and reply format are unchanged.`]
    : [...session(String(live.id)), `@${RUN}/prompts/${live.id}.md`];
  let child: ChildProcess | undefined;
  const ticker = setInterval(() => {
    const now = Date.now();
    // A long gap between ticks means the machine slept, which doesn't count either.
    if (now - live.updated > 30_000) live.deadline += now - live.updated;
    live.updated = now;
    if (now >= live.deadline && !live.timedOut) {
      live.timedOut = true;
      killTree(child!.pid!);
    }
    save(live);
  }, 5000);
  const r = await sh([...PI, ...message.slice(0, -1), "--tools", "read,bash,edit,write,time_left", message.at(-1)!], cwd,
    { CARGO_TARGET_DIR: target, SSCH_STATE: stateFile(live.id) }, undefined, c => (child = c));
  clearInterval(ticker);
  if (!live.timedOut) live.notes = r.code === 0 ? r.out.trim() : `agent exited ${r.code}: ${r.err.slice(-1000)}`;
  save(live);
}

async function finish(live: Live) {
  const { wt, cwd, target } = paths(live);
  // A run killed between recording and cleanup must not record the attempt twice.
  if (attempts().some(a => a.id === live.id)) return void cleanup(live);
  git(wt, "add", "-A");
  if (git(wt, "status", "--porcelain")) git(wt, "commit", "-qm", `ssch-research ${live.id}: uncommitted leftovers`);
  const entry: Attempt = { kind: "attempt", id: live.id, commit: git(wt, "rev-parse", "HEAD"), accepted: false, direction: live.direction, loc: loc(cwd), notes: live.notes ?? "" };
  if (live.timedOut) entry.error = "timeout: no submission within the time budget";
  else {
    log(`attempt ${live.id} judging ${entry.commit.slice(0, 8)}`);
    await judge(entry, cwd, target, live.id === 0);
  }
  record(entry);
  cleanup(live);
  log(`attempt ${live.id}: ${entry.accepted ? "NEW CHAMPION" : "rejected"} research=${entry.research?.total_gas ?? "-"} validation low=${tier(entry.validation_low)} high=${tier(entry.validation)}${entry.error ? ` error=${entry.error.split("\n")[0]}` : ""}`);
}

function cleanup(live: Live) {
  git(TOP, "worktree", "remove", "--force", paths(live).wt);
  rmSync(stateFile(live.id));
}

// Agents work in parallel; judging waits until all of them are done so no agent load skews the timing caps.
async function round(lives: Live[]) {
  await Promise.all(lives.filter(l => l.notes === undefined && !l.timedOut).map(research));
  for (const live of lives) await finish(live);
}

const tier = (t?: Tier) => t ? `${t.total_gas} gas/${t.wall_seconds.toFixed(2)}s` : "-";

function dashboardViews() {
  if (!existsSync(PER_GRAPH)) return [];
  const rows = readFileSync(PER_GRAPH, "utf8").trim().split("\n").slice(1).map(line => line.split("\t"))
    .map(([hash, ops, inputs, outputs, gas, best, gap]) => ({ hash: hash!, ops: +ops!, inputs: +inputs!, outputs: +outputs!, gas: +gas!, best: +best!, gap: +gap! }));
  const buckets = (key: "ops" | "inputs" | "outputs") => {
    const edges = [-1, 4, 8, 16, 32, Infinity];
    const lines = edges.slice(0, -1).map((low, i) => {
      const group = rows.filter(r => r[key] > low && r[key] <= edges[i + 1]!);
      return `${key} ${low + 1}-${edges[i + 1]}: ${group.length} graphs, ${group.filter(r => r.gap > 0).length} above best known, total gap ${group.reduce((s, r) => s + r.gap, 0)}`;
    });
    return `Gap by ${key}:\n${lines.join("\n")}`;
  };
  const table = (title: string, picked: typeof rows) => `${title}\nhash\tops\tinputs\toutputs\tgas\tbest\tgap\n${picked.map(r => [r.hash.slice(0, 17), r.ops, r.inputs, r.outputs, r.gas, r.best, r.gap].join("\t")).join("\n")}`;
  return [buckets("ops"), buckets("inputs"), buckets("outputs"),
    table("Worst 15 graphs:", rows.slice(0, 15)),
    table("15 random graphs above best known:", shuffle(rows.filter(r => r.gap > 0)).slice(0, 15))];
}

function shuffle<T>(items: T[]) {
  return items.map(item => [Math.random(), item] as const).sort((a, b) => a[0] - b[0]).map(([, item]) => item);
}

async function plan() {
  log(`stalled for ${stalled()} attempts; asking ${PLANNERS} planners`);
  const best = champion();
  let algorithm = "(none written yet)";
  try { algorithm = git(TOP, "show", `${best.commit}:plankc/sir/crates/stack-scheduling/ALGORITHM.md`); } catch {}
  const summaries = attempts().map(a => `- attempt ${a.id} ${a.commit.slice(0, 8)} ${a.accepted ? "ACCEPTED" : "rejected"} research=${a.research?.total_gas ?? "-"}${a.error ? ` error: ${a.error.split("\n")[0]}` : ""}\n  ${a.notes.slice(0, 600).replaceAll("\n", "\n  ")}`);
  const round = entries().filter(e => e.kind === "plan").length;
  const replies = await Promise.all(Array.from({ length: PLANNERS }, (_, i) => {
    const views = shuffle(dashboardViews()).slice(0, 2).join("\n\n");
    const archive = shuffle(summaries).slice(0, Math.ceil(summaries.length / 2)).join("\n");
    const prompt = `You plan research on the stack scheduler of an EVM compiler: it turns a basic block's operation graph into stack operations (dupN, swapN, pop, ops, spills), minimising gas. Gap = our gas minus the best-known schedule's gas for a graph.

Champion's ALGORITHM.md:
${algorithm}

Partial dashboard of the champion on the research set:
${views}

Part of the archive of previous attempts:
${archive}

Propose ${DIRECTIONS} distinct research directions, each with your probability that it beats the champion. Exactly one per line, formatted \`p=0.NN | direction\`, nothing else.`;
    return sh([...PI, ...session(`plan-${round}-${i}`), "--no-tools", prompt], RUN);
  }));
  const proposals = replies.flatMap(r => r.out.split("\n")).map(l => l.trim()).filter(l => l.startsWith("p="));
  const dedup = await sh([...PI, ...session(`plan-${round}-dedup`), "--no-tools", `Remove near-duplicates from these research directions, keeping the clearer wording. Output the remaining lines unchanged, one per line, nothing else.\n\n${proposals.join("\n")}`], RUN);
  const directions = dedup.out.split("\n").map(l => l.trim()).filter(l => l.startsWith("p="));
  record({ kind: "plan", directions: directions.length ? directions : proposals });
  log(`planned ${directions.length} directions`);
}

mkdirSync(`${RUN}/prompts`, { recursive: true });
mkdirSync(`${RUN}/logs`, { recursive: true });
mkdirSync(`${RUN}/wt`, { recursive: true });
killOrphans();
const inProgress = readdirSync(`${RUN}/wt`).filter(f => f.endsWith(".json")).map(f => JSON.parse(readFileSync(`${RUN}/wt/${f}`, "utf8")) as Live);
for (const dir of readdirSync(`${RUN}/wt`).filter(f => !f.endsWith(".json") && !inProgress.some(l => String(l.id) === f))) {
  log(`discarding incomplete setup ${dir}`);
  rmSync(`${RUN}/wt/${dir}`, { recursive: true, force: true });
}
git(TOP, "worktree", "prune");
const known = new Set([...attempts(), ...inProgress].map(a => String(a.id)));
for (const branch of git(TOP, "branch", "--list", "--format=%(refname:short)", `${BRANCH}/*`).split("\n").filter(Boolean)) {
  const id = branch.split("/").at(-1)!;
  if (/^\d+$/.test(id) && !known.has(id)) git(TOP, "branch", "-D", branch);
}

async function guarded(lives: Live[]) {
  try { await round(lives); } catch (error) {
    log(`round crashed, stopping: ${error}`);
    stopping = true;
  }
}

if (inProgress.length) {
  log(`resuming attempts ${inProgress.map(l => l.id).join(", ")}`);
  await guarded(inProgress.sort((a, b) => a.id - b.id));
}
if (!attempts().length) {
  log("judging the baseline (attempt 0)");
  await round([prepare(0, 0)]);
  if (!champion()) throw new Error(`baseline failed: ${attempts()[0]!.error}`);
}

let nextId = Math.max(...attempts().map(a => a.id)) + 1;
while (!stopping && Date.now() < DEADLINE) {
  const size = Math.min(WORKERS, MAX_ATTEMPTS - (attempts().length - 1));
  if (size <= 0) break;
  if (stalled() >= STALL && !pendingDirections().length) { await plan(); continue; }
  const directions = pendingDirections();
  await guarded(Array.from({ length: size }, (_, slot) => prepare(nextId++, slot, directions[slot])));
}
log(`stopped; champion ${champion().commit} validation=${champion().validation!.total_gas}. Rerun the same command to resume.`);
