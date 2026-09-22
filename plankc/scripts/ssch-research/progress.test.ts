import { expect, test } from "bun:test";
import { progress } from "./progress";

test("progress follows tool activity, idle streaming, retry and completion", () => {
  let now = 0;
  const lines: string[] = [];
  const monitor = progress("judge", line => lines.push(line), () => now);
  const event = (value: unknown) => monitor.push(Buffer.from(JSON.stringify(value) + "\n"));
  event({ type: "tool_execution_start", toolCallId: "1", toolName: "bash", args: { command: "bench --evaluate validation.sqlite3 512" } });
  now = 30_000;
  monitor.heartbeat();
  event({ type: "tool_execution_end", toolCallId: "1", toolName: "bash", isError: false });
  now = 35_000;
  event({ type: "message_update", assistantMessageEvent: { type: "thinking_delta", delta: "private reasoning" } });
  now = 40_000;
  monitor.heartbeat();
  event({ type: "auto_retry_start" });
  event({ type: "message_end", message: { role: "assistant", stopReason: "stop", content: [{ type: "text", text: "Accepted." }] } });
  expect(monitor.finish()?.stopReason).toBe("stop");
  expect(lines).toEqual([
    "judge ▶ bash: bench --evaluate validation.sqlite3 512",
    "judge heartbeat: elapsed 30s, last event 30s ago; bash: bench --evaluate validation.sqlite3 512 (30s)",
    "judge ✓ bash (30s)",
    "judge heartbeat: elapsed 40s, last event 5s ago; waiting for model/API (no active tool)",
    "judge: auto_retry_start",
    "judge: Accepted.",
  ]);
});

test("progress handles split JSON, split UTF-8 and a final line without LF", () => {
  const lines: string[] = [];
  const monitor = progress("research", line => lines.push(line));
  const bytes = Buffer.from(JSON.stringify({ type: "message_end", message: { role: "assistant", stopReason: "error", content: [{ type: "text", text: "café" }] } }));
  for (const byte of bytes) monitor.push(Buffer.from([byte]));
  expect(monitor.finish()?.stopReason).toBe("error");
  expect(lines).toEqual(["research: café"]);
});
