import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { load, save, type Manifest } from "./data";

export interface ChampionBaseline {
  commit: string;
  efforts: number[];
  researchSha256: string;
  validationSha256: string;
  validationGas: number[];
  researchWallSeconds: number[];
  validationWallSeconds: number[];
}

interface Measurement {
  returncode: number;
  wall_seconds: number;
  result: { graphs: number; effort: number; total_gas: number };
}

export function measurements(directory: string, efforts: number[], expectedGraphs: number, implementation = "candidate", dataset = "validation") {
  return efforts.map(effort => {
    const runs = [1, 2].map(repetition => {
      const file = resolve(directory, `${implementation}-${dataset}-${effort}-${repetition}.measurement.json`);
      if (!existsSync(file)) throw new Error(`Missing measured benchmark evidence: ${file}`);
      const run = load<Measurement>(file);
      if (run.returncode !== 0 || run.result?.effort !== effort || run.result.graphs !== expectedGraphs ||
        !Number.isSafeInteger(run.result.total_gas) || run.result.total_gas < 0 ||
        !Number.isFinite(run.wall_seconds) || run.wall_seconds < 0) {
        throw new Error(`Invalid benchmark evidence: ${file}`);
      }
      return run;
    });
    if (runs[0]!.result.total_gas !== runs[1]!.result.total_gas) throw new Error(`Inconsistent gas at effort ${effort}`);
    return runs;
  });
}

function acceptedEvidence(directory: string, commit: string, acceptedRound: number): string {
  for (let n = acceptedRound; n >= 1; n--) {
    const candidate = resolve(directory, `round-${n}`);
    const outcomePath = resolve(candidate, "outcome.json");
    if (!existsSync(outcomePath)) continue;
    const outcome = load<{ champion: string; accepted: boolean }>(outcomePath);
    if (outcome.accepted && outcome.champion === commit) return candidate;
  }
  throw new Error("No accepted-round evidence matches champion");
}

export function baseline(directory: string, manifest: Manifest, commit: string, efforts: number[], acceptedRound: number): ChampionBaseline {
  const path = resolve(directory, "champion-baseline.json");
  if (existsSync(path)) {
    const previous = load<ChampionBaseline>(path);
    if (previous.commit !== commit || JSON.stringify(previous.efforts) !== JSON.stringify(efforts) ||
      previous.researchSha256 !== manifest.files.research!.sha256 || previous.validationSha256 !== manifest.files.validation!.sha256) {
      throw new Error("Champion baseline does not match saved commit, efforts or datasets");
    }
    if (previous.validationWallSeconds === undefined) {
      if (acceptedRound < 1) throw new Error("Champion baseline needs validation wall times");
      const evidence = acceptedEvidence(directory, commit, acceptedRound);
      previous.validationWallSeconds = measurements(evidence, efforts, manifest.files.validation!.count!)
        .map(runs => Math.max(...runs.map(run => run.wall_seconds)));
      save(path, previous);
    }
    return previous;
  }
  if (acceptedRound < 1) throw new Error("Missing champion baseline; measure champion once before judging");
  const round = acceptedEvidence(directory, commit, acceptedRound);
  const validation = measurements(round, efforts, manifest.files.validation!.count!);
  const research = measurements(round, efforts, manifest.files.research!.count!, "candidate", "research");
  const result: ChampionBaseline = {
    commit, efforts, researchSha256: manifest.files.research!.sha256,
    validationSha256: manifest.files.validation!.sha256,
    validationGas: validation.map(runs => runs[0]!.result.total_gas),
    researchWallSeconds: research.map(runs => Math.max(...runs.map(run => run.wall_seconds))),
    validationWallSeconds: validation.map(runs => Math.max(...runs.map(run => run.wall_seconds))),
  };
  save(path, result);
  return result;
}

export function qualifies(championGas: number, candidateGas: number): boolean {
  if (!Number.isSafeInteger(championGas) || championGas <= 0 || !Number.isSafeInteger(candidateGas) || candidateGas < 0) throw new Error("Invalid gas totals");
  return BigInt(candidateGas) * 1000n <= BigInt(championGas) * 997n;
}

export function saveBaseline(directory: string, manifest: Manifest, commit: string, efforts: number[], round: number) {
  const path = resolve(directory, `round-${round}`);
  const validation = measurements(path, efforts, manifest.files.validation!.count!);
  const research = measurements(path, efforts, manifest.files.research!.count!, "candidate", "research");
  save(resolve(directory, "champion-baseline.json"), {
    commit, efforts, researchSha256: manifest.files.research!.sha256,
    validationSha256: manifest.files.validation!.sha256,
    validationGas: validation.map(runs => runs[0]!.result.total_gas),
    researchWallSeconds: research.map(runs => Math.max(...runs.map(run => run.wall_seconds))),
    validationWallSeconds: validation.map(runs => Math.max(...runs.map(run => run.wall_seconds))),
  } satisfies ChampionBaseline);
}
