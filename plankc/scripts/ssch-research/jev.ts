#!/usr/bin/env bun
import { readFileSync, writeFileSync } from "node:fs";

const [input, output] = process.argv.slice(2);
if (!input || !output) throw new Error("Usage: bun jev.ts REQUEST.json OUTPUT.json");
const key = process.env.TYPESAFE_API_KEY;
if (!key) throw new Error("Set TYPESAFE_API_KEY (Bun loads .env from the invoking project)");
const request = { ...JSON.parse(readFileSync(input, "utf8")), model: "jev-1.13.0" };
if (request.state === undefined || !request.questions) throw new Error("Request requires state and questions");
const started = performance.now();
for (let attempt = 0; ; attempt++) {
  const response = await fetch("https://api.typesafe.ai/v1/systemone", {
    method: "POST",
    headers: { Authorization: `Bearer ${key}`, "Content-Type": "application/json" },
    body: JSON.stringify(request),
    signal: AbortSignal.timeout(120_000),
  });
  const body = await response.text();
  if (!response.ok) {
    if (attempt < 5 && (response.status === 429 || response.status >= 500)) {
      const retry = response.headers.get("retry-after");
      const delay = retry ? (Number.isFinite(Number(retry)) ? Number(retry) * 1000 : Date.parse(retry) - Date.now()) : 0;
      await Bun.sleep(Math.max(Number.isFinite(delay) ? delay : 0, 1000 * 2 ** attempt));
      continue;
    }
    throw new Error(`TypeSafe HTTP ${response.status}; request failed (credentials omitted)`);
  }
  const record = { request, response: JSON.parse(body), elapsedMs: performance.now() - started, attempts: attempt + 1, timestamp: new Date().toISOString() };
  writeFileSync(output, JSON.stringify(record, null, 2) + "\n", { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify({ output, model: record.response.model, usage: record.response.usage, elapsedMs: record.elapsedMs }));
  break;
}
