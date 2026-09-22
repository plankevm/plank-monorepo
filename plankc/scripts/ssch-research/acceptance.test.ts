import { expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { baseline, measurements, qualifies, saveBaseline } from "./acceptance";
import { save, type Manifest } from "./data";

const manifest: Manifest = {
  source: "/source", seed: "test", removeBeforeResearch: [],
  files: {
    research: { path: "/research", sha256: "research-sha", count: 70 },
    validation: { path: "/validation", sha256: "validation-sha", count: 15 },
  },
};

function record(directory: string, dataset: string, effort: number, repetition: number, gas: number) {
  save(resolve(directory, `candidate-${dataset}-${effort}-${repetition}.measurement.json`), {
    returncode: 0, wall_seconds: 3, result: { effort, graphs: dataset === "research" ? 70 : 15, total_gas: gas },
  });
}

test("0.3% floor is inclusive, integer exact and relative to champion gas", () => {
  expect(qualifies(34437, 34334)).toBe(false);
  expect(qualifies(34437, 34333)).toBe(true);
  expect(qualifies(1000, 997)).toBe(true);
  expect(qualifies(1000, 998)).toBe(false);
});

test("accepted candidate becomes durable champion baseline, invalid evidence pauses", () => {
  const directory = mkdtempSync(resolve(tmpdir(), "ssch-acceptance-"));
  try {
    const round = resolve(directory, "round-1");
    save(resolve(round, "outcome.json"), { champion: "candidate-commit", accepted: true });
    for (const dataset of ["research", "validation"]) for (const effort of [1, 2, 3]) for (const repetition of [1, 2]) {
      record(round, dataset, effort, repetition, 997 - effort);
    }
    expect(measurements(round, [1, 2, 3], 15).map(runs => runs[0]!.result.total_gas)).toEqual([996, 995, 994]);
    saveBaseline(directory, manifest, "candidate-commit", [1, 2, 3], 1);
    expect(baseline(directory, manifest, "candidate-commit", [1, 2, 3], 1).validationGas).toEqual([996, 995, 994]);
    expect(() => baseline(directory, manifest, "other-commit", [1, 2, 3], 1)).toThrow();
    expect(() => baseline(directory, manifest, "candidate-commit", [1, 2, 4], 1)).toThrow();
    record(round, "validation", 3, 2, 990);
    expect(() => measurements(round, [1, 2, 3], 15)).toThrow("Inconsistent");
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test("legacy accepted round-10 evidence bootstraps cache without rerunning champion", () => {
  const directory = mkdtempSync(resolve(tmpdir(), "ssch-bootstrap-"));
  try {
    const round = resolve(directory, "round-10");
    save(resolve(round, "outcome.json"), { champion: "accepted-commit", accepted: true });
    for (const dataset of ["research", "validation"]) for (const effort of [512, 8192, 131072]) for (const repetition of [1, 2]) {
      record(round, dataset, effort, repetition, 34437);
    }
    save(resolve(directory, "round-11/outcome.json"), { champion: "accepted-commit", accepted: false });
    expect(baseline(directory, manifest, "accepted-commit", [512, 8192, 131072], 11).validationGas).toEqual([34437, 34437, 34437]);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
