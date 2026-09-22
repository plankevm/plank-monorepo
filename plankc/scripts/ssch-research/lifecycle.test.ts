import { test, expect } from "bun:test";
import { Database } from "bun:sqlite";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";

test("resume after API failure, checkpoint winner, retain rejected candidates, stop after three failures", () => {
  const temp = mkdtempSync(resolve(tmpdir(), "ssch-lifecycle-"));
  try {
    const repo = resolve(temp, "repo");
    const project = resolve(repo, "plankc");
    const scripts = resolve(project, "scripts/ssch-research");
    const source = resolve(project, "corpus/stack-scheduling-db/canonical-blocks.sqlite3");
    mkdirSync(scripts, { recursive: true });
    mkdirSync(resolve(project, "sir/crates/stack-scheduling/src"), { recursive: true });
    mkdirSync(resolve(project, "corpus/stack-scheduling-db"), { recursive: true });
    writeFileSync(resolve(project, ".gitignore"), "corpus/\ntmp/\n");
    writeFileSync(resolve(project, "sir/crates/stack-scheduling/src/scheduler.rs"), "initial\n");
    for (const name of ["data.ts", "run.ts", "typesafe-skill.md"]) copyFileSync(resolve(import.meta.dir, name), resolve(scripts, name));
    const db = new Database(source);
    db.exec("PRAGMA user_version=2; CREATE TABLE canonical_blocks (canonical_hash TEXT PRIMARY KEY, canonical_graph TEXT NOT NULL, best_schedule TEXT NOT NULL, best_gas_cost INTEGER NOT NULL, manually_optimized INTEGER NOT NULL)");
    for (let i = 0; i < 100; i++) db.prepare("INSERT INTO canonical_blocks VALUES (?, ?, '[]', 0, 0)").run(`hash${i}`, JSON.stringify({ fixture: i }));
    db.close();
    function command(args: string[], env = process.env) {
      return Bun.spawnSync(args, { cwd: project, env, stdout: "pipe", stderr: "pipe" });
    }
    for (const args of [["init", repo], ["config", "user.name", "Test"], ["config", "user.email", "test@example.invalid"], ["add", "."], ["commit", "-m", "fixture"]]) {
      expect(command(["git", ...args]).exitCode).toBe(0);
    }
    const privateDir = resolve(temp, "private");
    const runner = resolve(scripts, "run.ts");
    expect(command([process.execPath, runner, "prepare", privateDir, "--writers-stopped"]).exitCode).toBe(0);
    const manifest = JSON.parse(readFileSync(resolve(privateDir, "manifest.json"), "utf8"));
    expect(command([process.execPath, runner, "activate", privateDir, "--archive-confirmed"]).exitCode).not.toBe(0);
    for (const path of manifest.removeBeforeResearch) rmSync(path, { force: true });
    expect(command([process.execPath, runner, "activate", privateDir, "--archive-confirmed"]).exitCode).toBe(0);
    const bin = resolve(temp, "bin");
    mkdirSync(bin);
    const mock = resolve(bin, "pi");
    writeFileSync(mock, `#!${process.execPath}
import {readFileSync, writeFileSync, existsSync, appendFileSync} from 'node:fs';
const prompt = readFileSync(process.argv.at(-1).slice(1), 'utf8');
if (!existsSync(process.env.MOCK_API_MARKER)) {
  writeFileSync(process.env.MOCK_API_MARKER, 'failed once');
  process.exit(1);
}
const output = prompt.match(/Write (.+?\\.json) as/)[1];
if (prompt.startsWith('You are researching')) {
  appendFileSync('sir/crates/stack-scheduling/src/scheduler.rs', 'candidate\\n');
  writeFileSync(output, JSON.stringify({efforts:[1,2,3],summary:'fixture'}));
} else {
  writeFileSync(output, JSON.stringify({accept:output.includes('/round-1/'),feedback:'aggregate fixture',evidence:'private fixture',championEfforts:[2,3,4]}));
}
console.log(JSON.stringify({type:'message_end',message:{role:'assistant',stopReason:'stop',content:[]}}));
`);
    chmodSync(mock, 0o755);
    const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, MOCK_API_MARKER: resolve(temp, "api-marker") };
    const first = command([process.execPath, runner, "run", privateDir], env);
    expect(first.exitCode).not.toBe(0);
    let state = JSON.parse(readFileSync(resolve(privateDir, "state.json"), "utf8"));
    expect([state.round, state.failures, state.phase]).toEqual([1, 0, "research"]);
    expect(command(["git", "worktree", "remove", "--force", state.worktree]).exitCode).toBe(0);
    const resumed = command([process.execPath, runner, "run", privateDir], env);
    if (resumed.exitCode) {
      const notes = resolve(state.worktree, "plankc/tmp/ssch-research/round-1");
      throw new Error(resumed.stderr.toString() + readdirSync(notes).filter(n => n.endsWith(".stderr")).map(n => readFileSync(resolve(notes, n), "utf8")).join("\n"));
    }
    state = JSON.parse(readFileSync(resolve(privateDir, "state.json"), "utf8"));
    expect([state.round, state.failures, state.phase]).toEqual([5, 3, "research"]);
    expect(state.championEfforts).toEqual([2, 3, 4]);
    const head = command(["git", "-C", state.worktree, "rev-parse", "HEAD"]).stdout.toString().trim();
    expect(head).toBe(state.champion);
    expect(readFileSync(resolve(state.worktree, "plankc/sir/crates/stack-scheduling/src/scheduler.rs"), "utf8")).toBe("initial\ncandidate\n");
    const refs = command(["git", "for-each-ref", "--format=%(refname)", "refs/ssch-research"]).stdout.toString().trim().split("\n");
    expect(refs.length).toBe(4);
    expect(command(["git", "status", "--porcelain"]).stdout.toString()).toBe("");
  } finally { rmSync(temp, { recursive: true, force: true }); }
}, 30_000);
