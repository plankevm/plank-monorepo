import { Database } from "bun:sqlite";
import { createHash, randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, writeFileSync } from "node:fs";
import { dirname, resolve, relative, isAbsolute } from "node:path";

export const ROOT = resolve(import.meta.dir, "../..");
export const CORPUS = resolve(ROOT, "corpus/stack-scheduling-db");
export function save(path: string, value: unknown) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(`${path}.tmp`, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
  renameSync(`${path}.tmp`, path);
}
export function load<T>(path: string): T { return JSON.parse(readFileSync(path, "utf8")); }
export function sha(path: string) { return createHash("sha256").update(readFileSync(path)).digest("hex"); }
export function outside(root: string, path: string) {
  const rel = relative(root, path);
  return rel === ".." || rel.startsWith("../") || isAbsolute(rel);
}

type Row = { canonical_hash: string; canonical_graph: string; best_schedule: string; best_gas_cost: number; manually_optimized: number };
export function partition(rows: Row[], seed: string): Row[][] {
  // Canonical identity is the only provenance available in this database.
  const groups = new Map<string, Row[]>();
  for (const row of rows) {
    const key = JSON.stringify(JSON.parse(row.canonical_graph));
    const group = groups.get(key) ?? [];
    group.push(row);
    groups.set(key, group);
  }
  const ordered = [...groups.entries()].sort(([a], [b]) => {
    const hash = (s: string) => createHash("sha256").update(seed).update(s).digest("hex");
    return hash(a).localeCompare(hash(b));
  });
  const parts: Row[][] = [[], [], []];
  let count = 0;
  for (const [, group] of ordered) {
    const index = count < rows.length * .7 ? 0 : count < rows.length * .85 ? 1 : 2;
    parts[index]!.push(...group);
    count += group.length;
  }
  if (parts.some(p => p.length === 0)) throw new Error("Not enough distinct graphs for three nonempty partitions");
  return parts;
}

function createDatabase(path: string, rows: Row[], schema: string) {
  if (existsSync(path)) throw new Error(`Refusing to replace ${path}`);
  const db = new Database(path, { create: true });
  try {
    db.exec(schema);
    db.exec("PRAGMA user_version = 2");
    const insert = db.prepare("INSERT INTO canonical_blocks VALUES (?, ?, ?, ?, ?)");
    db.transaction(() => {
      for (const r of rows) insert.run(r.canonical_hash, r.canonical_graph, r.best_schedule, r.best_gas_cost, r.manually_optimized);
    })();
    if ((db.query("PRAGMA integrity_check").get() as any).integrity_check !== "ok") throw new Error("Split integrity check failed");
  } finally { db.close(); }
}

export interface Manifest {
  source: string;
  seed: string;
  files: Record<string, { path: string; sha256: string; count?: number }>;
  removeBeforeResearch: string[];
  acknowledged?: boolean;
}

export function prepare(directory: string, source = resolve(CORPUS, "canonical-blocks.sqlite3"), seed = randomBytes(32).toString("hex"), inventory = dirname(source)) {
  directory = resolve(directory);
  if (!outside(ROOT, directory)) throw new Error("Private directory must be outside plankc");
  if (existsSync(directory)) throw new Error("Preparation requires a new directory");
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const full = resolve(directory, "full-backup.sqlite3");
  const original = new Database(source, { readonly: true });
  try {
    original.exec(`VACUUM INTO '${full.replaceAll("'", "''")}'`);
  } finally { original.close(); }
  const snapshot = new Database(full, { readonly: true });
  let rows: Row[];
  let schema: string;
  try {
    if ((snapshot.query("PRAGMA integrity_check").get() as any).integrity_check !== "ok") throw new Error("Backup integrity check failed");
    if ((snapshot.query("PRAGMA user_version").get() as any).user_version !== 2) throw new Error("Expected schema v2");
    rows = snapshot.query("SELECT * FROM canonical_blocks ORDER BY canonical_hash").all() as Row[];
    schema = (snapshot.query("SELECT sql FROM sqlite_master WHERE name = 'canonical_blocks'").get() as { sql: string }).sql;
  } finally { snapshot.close(); }
  const parts = partition(rows, seed);
  const files: Manifest["files"] = { full: { path: full, sha256: sha(full), count: rows.length } };
  for (const [i, name] of ["research", "validation", "final-test"].entries()) {
    const path = resolve(directory, `${name}.sqlite3`);
    createDatabase(path, parts[i]!, schema);
    files[name] = { path, sha256: sha(path), count: parts[i]!.length };
  }
  const removeBeforeResearch = [...new Set([
    source, full, files["final-test"]!.path,
    ...(existsSync(inventory) ? readdirSync(inventory) : []).filter(n => /\.sqlite3|\.csv/.test(n)).map(n => resolve(inventory, n)),
  ])];
  const manifest: Manifest = { source, seed, files, removeBeforeResearch };
  save(resolve(directory, "manifest.json"), manifest);
  save(resolve(directory, "split-summary.json"), Object.fromEntries(parts.map((part, i) => [["research", "validation", "final-test"][i], {
    count: part.length,
    bestKnownGas: part.reduce((n, r) => n + r.best_gas_cost, 0),
    graphBytes: part.reduce((n, r) => n + r.canonical_graph.length, 0),
    manuallyOptimized: part.filter(r => r.manually_optimized).length,
  }])));
  writeFileSync(resolve(directory, "ARCHIVE-CHECKLIST.md"), `# STOP: archive before research\n\nNo original data was modified. Writers must have been stopped BEFORE preparation and must remain stopped through activation. If any writes occurred since preparation, DO NOT delete originals: prepare a fresh snapshot and repeat archive verification. Otherwise recent writes would be lost.\n\nUpload and verify full-backup.sqlite3, final-test.sqlite3, manifest.json, and split-summary.json.\nAlso archive the existing corpus CSVs, older DB backups, and run history if wanted.\nUse SQLite online backups for any live databases; never copy a live DB without its WAL.\n\nThen remove ALL the following local originals/copies (including sidecars):\n\n${removeBeforeResearch.map(p => `- \`${p}\``).join("\n")}\n\nKeep research.sqlite3 and validation.sqlite3 here. Do not upload credentials.\nSearch for any other full-data/final-test copies, exports, caches, old worktrees, and backups; this inventory only covers the known corpus directory. Source corpora can reconstruct cases: researchers must not use them.\n\nAfter verifying cloud checksums and completing removal, run:\n\n\`bun scripts/ssch-research/run.ts activate ${JSON.stringify(directory)} --archive-confirmed\`\n\nThis installs the research-only DB at the original path. No final-test queries will be run locally.\nSplitting groups identical canonical graphs; source-family provenance is unavailable, so related nonidentical graphs may cross partitions.\n`);
  console.log(`Prepared ${directory}\nRead ARCHIVE-CHECKLIST.md. Research has NOT started.`);
}

export function activate(directory: string) {
  const path = resolve(directory, "manifest.json");
  const m = load<Manifest>(path);
  if (m.acknowledged) throw new Error("Already activated");
  const installed = existsSync(m.source) && sha(m.source) === m.files.research!.sha256;
  const remaining = m.removeBeforeResearch.filter(p => !(p === m.source && installed) && existsSync(p));
  if (remaining.length) throw new Error(`Archive/removal incomplete:\n${remaining.join("\n")}`);
  for (const name of ["research", "validation"]) {
    const file = m.files[name]!;
    if (sha(file.path) !== file.sha256) throw new Error(`${name} checksum mismatch`);
  }
  mkdirSync(dirname(m.source), { recursive: true });
  if (!installed) writeFileSync(m.source, readFileSync(m.files.research!.path), { flag: "wx" });
  m.acknowledged = true;
  save(path, m);
  console.log("Research-only canonical DB installed. Commit the implementation before running research.");
}
