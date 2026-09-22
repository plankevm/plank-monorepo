import { describe, expect, test } from "bun:test";
import { Database } from "bun:sqlite";
import { mkdtempSync, existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { partition, prepare, sha, load, type Manifest } from "./data";
import { allowed, efforts } from "./run";

const rows = Array.from({ length: 100 }, (_, i) => ({
  canonical_hash: `hash-${i}`, canonical_graph: JSON.stringify({ fixture: i }),
  best_schedule: "[]", best_gas_cost: i, manually_optimized: i % 2,
}));

describe("research protocol", () => {
  test("split is deterministic, disjoint, complete, and approximately 70/15/15", () => {
    const parts = partition(rows, "seed");
    expect(parts.map(p => p.length)).toEqual([70, 15, 15]);
    expect(partition([...rows].reverse(), "seed")).toEqual(parts);
    expect(new Set(parts.flat().map(r => r.canonical_hash)).size).toBe(100);
    expect(partition(rows, "other seed")).not.toEqual(parts);
  });
  test("identical canonical graphs never cross partitions", () => {
    const duplicated = [...rows, { ...rows[0]!, canonical_hash: "duplicate" }];
    const parts = partition(duplicated, "seed");
    expect(parts.filter(p => p.some(r => r.canonical_graph === rows[0]!.canonical_graph)).length).toBe(1);
  });
  test("rejects insufficient data", () => {
    expect(() => partition(rows.slice(0, 1), "seed")).toThrow();
  });
  test("efforts reject invalid submissions", () => {
    for (const v of [[], [1, 2], [0, 2, 3], [3, 2, 1], [1, 2.5, 3], [1, 2, Infinity], ["1", 2, 3]]) expect(() => efforts(v)).toThrow();
    expect(() => efforts([1, 2, 3])).not.toThrow();
    expect(() => efforts([1, 1, 1])).not.toThrow();
  });
  test("frozen evaluator and validation paths are not writable scope", () => {
    expect(allowed("plankc/sir/crates/stack-scheduling/src/scheduler.rs")).toBe(true);
    for (const p of ["validation.rs", "display.rs", "stack.rs", "stack_ops.rs", "op_graph/mod.rs"]) {
      expect(allowed(`plankc/sir/crates/stack-scheduling/src/${p}`)).toBe(false);
    }
    expect(allowed("plankc/devtools/stack-scheduling-db-bench/src/main.rs")).toBe(false);
  });
  test("preparation snapshots WAL data without changing the original and pauses", () => {
    const temp = mkdtempSync(resolve(tmpdir(), "ssch-test-"));
    const source = resolve(temp, "source.sqlite3");
    const db = new Database(source);
    try {
      db.exec("PRAGMA journal_mode=WAL; PRAGMA user_version=2; CREATE TABLE canonical_blocks (canonical_hash TEXT PRIMARY KEY, canonical_graph TEXT NOT NULL, best_schedule TEXT NOT NULL, best_gas_cost INTEGER NOT NULL, manually_optimized INTEGER NOT NULL)");
      const insert = db.prepare("INSERT INTO canonical_blocks VALUES (?, ?, ?, ?, ?)");
      for (const r of rows) insert.run(r.canonical_hash, r.canonical_graph, r.best_schedule, r.best_gas_cost, r.manually_optimized);
      const before = sha(source);
      const directory = resolve(temp, "private");
      prepare(directory, source, "seed");
      const m = load<Manifest>(resolve(directory, "manifest.json"));
      expect(m.acknowledged).toBeUndefined();
      expect(sha(source)).toBe(before);
      expect(m.files.full!.count).toBe(100);
      expect(["research", "validation", "final-test"].map(n => m.files[n]!.count)).toEqual([70, 15, 15]);
      for (const f of Object.values(m.files)) expect(sha(f.path)).toBe(f.sha256);
      expect(existsSync(resolve(directory, "ARCHIVE-CHECKLIST.md"))).toBe(true);
      expect(() => prepare(directory, source, "seed")).toThrow();
      expect(readFileSync(source).length).toBeGreaterThan(0);
    } finally { db.close(); rmSync(temp, { recursive: true, force: true }); }
  });
});
