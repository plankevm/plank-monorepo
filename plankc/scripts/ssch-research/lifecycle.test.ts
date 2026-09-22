import { test, expect } from "bun:test";
import { Database } from "bun:sqlite";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { save, sha } from "./data";
import { saveBaseline } from "./acceptance";

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
    for (const name of ["acceptance.ts", "data.ts", "run.ts", "progress.ts", "typesafe-skill.md"]) copyFileSync(resolve(import.meta.dir, name), resolve(scripts, name));
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
    writeFileSync(resolve(privateDir, "champion-baseline.json"), JSON.stringify({
      commit: command(["git", "rev-parse", "HEAD"]).stdout.toString().trim(),
      efforts: [4000, 16000, 96000], researchSha256: sha(manifest.files.research.path),
      validationSha256: sha(manifest.files.validation.path),
      validationGas: [100, 100, 100], researchWallSeconds: [1, 2, 3], validationWallSeconds: [1, 2, 3],
    }));
    const bin = resolve(temp, "bin");
    mkdirSync(bin);
    const mock = resolve(bin, "pi");
    writeFileSync(mock, `#!${process.execPath}
import {readFileSync, writeFileSync, existsSync, appendFileSync} from 'node:fs';
const prompt = readFileSync(process.argv.at(-1).slice(1), 'utf8');
appendFileSync(process.env.MOCK_ARGS_FILE, JSON.stringify(process.argv.slice(2)) + '\\n');
if (!existsSync(process.env.MOCK_API_MARKER)) {
  writeFileSync(process.env.MOCK_API_MARKER, 'failed once');
  process.exit(1);
}
const output = prompt.match(/Write (.+?\\.json) as/)[1];
if (prompt.startsWith('You are researching')) {
  appendFileSync('sir/crates/stack-scheduling/src/scheduler.rs', 'candidate\\n');
  writeFileSync(output, JSON.stringify({efforts:[4000,16000,96000],summary:'fixture'}));
} else {
  for (const dataset of ['research','validation']) for (const effort of [4000,16000,96000]) for (const repetition of [1,2]) {
    writeFileSync(output.replace('verdict.json', 'candidate-'+dataset+'-'+effort+'-'+repetition+'.measurement.json'),
      JSON.stringify({returncode:0,wall_seconds:1,result:{effort,graphs:dataset==='research'?70:15,total_gas:90}}));
  }
  writeFileSync(output, JSON.stringify({accept:true,feedback:'aggregate fixture',evidence:'private fixture',championEfforts:[4000,16000,96000]}));
}
console.log(JSON.stringify({type:'message_end',message:{role:'assistant',stopReason:'stop',content:[]}}));
`);
    chmodSync(mock, 0o755);
    const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, MOCK_API_MARKER: resolve(temp, "api-marker"), MOCK_ARGS_FILE: resolve(temp, "agent-args.jsonl") };
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
    expect(state.championEfforts).toEqual([4000, 16000, 96000]);
    const agentArgs: string[][] = readFileSync(env.MOCK_ARGS_FILE, "utf8").trim().split("\n").map(line => JSON.parse(line));
    const modelAndEffort = (args: string[]) => [args[args.indexOf("--model") + 1], args[args.indexOf("--thinking") + 1]];
    expect(modelAndEffort(agentArgs[1]!)).toEqual(["openai-codex/gpt-6-sol", "high"]);
    expect(modelAndEffort(agentArgs[2]!)).toEqual(["openai-codex/gpt-6-astra", "high"]);
    expect(JSON.parse(readFileSync(resolve(privateDir, "round-1/verdict.json"), "utf8")).accept).toBe(true);
    for (let round = 2; round <= 4; round++) {
      const verdict = JSON.parse(readFileSync(resolve(privateDir, `round-${round}/verdict.json`), "utf8"));
      expect(verdict.accept).toBe(false);
    }
    expect(JSON.parse(readFileSync(resolve(privateDir, "champion-baseline.json"), "utf8")).commit).toBe(state.champion);
    const head = command(["git", "-C", state.worktree, "rev-parse", "HEAD"]).stdout.toString().trim();
    expect(head).toBe(state.champion);
    expect(readFileSync(resolve(state.worktree, "plankc/sir/crates/stack-scheduling/src/scheduler.rs"), "utf8")).toBe("initial\ncandidate\n");
    const refs = command(["git", "for-each-ref", "--format=%(refname)", "refs/ssch-research"]).stdout.toString().trim().split("\n");
    expect(refs.length).toBe(4);
    expect(command(["git", "status", "--porcelain"]).stdout.toString()).toBe("");

    const pending = JSON.parse(readFileSync(resolve(privateDir, "round-4/outcome.json"), "utf8")).candidate;
    saveBaseline(privateDir, manifest, pending, [4000, 16000, 96000], 4);
    save(resolve(privateDir, "state.json"), {
      ...state, round: 4, phase: "promote", accepted: true, candidate: pending, failures: 2,
    });
    const recovered = command([process.execPath, runner, "run", privateDir], env);
    if (recovered.exitCode) throw new Error(recovered.stderr.toString());
    const after = JSON.parse(readFileSync(resolve(privateDir, "state.json"), "utf8"));
    expect(after.champion).toBe(pending);
    expect(after.round).toBe(8);
    expect(after.failures).toBe(3);
    expect(command(["git", "-C", after.worktree, "rev-parse", "HEAD"]).stdout.toString().trim()).toBe(pending);
  } finally { rmSync(temp, { recursive: true, force: true }); }
}, 30_000);
