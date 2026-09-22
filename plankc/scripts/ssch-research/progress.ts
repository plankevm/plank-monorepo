import { StringDecoder } from "node:string_decoder";

export function log(message: string) {
  console.log(`[${new Date().toISOString()}] ${message}`);
}

export function progress(role: string, emit = log, now = Date.now) {
  const started = now();
  let lastEvent = started;
  const active = new Map<string, { label: string; started: number }>();
  const decoder = new StringDecoder("utf8");
  let pending = "";
  let lastAssistant: { stopReason?: string } | undefined;
  const seconds = (start: number) => `${Math.round((now() - start) / 1000)}s`;
  function event(e: any) {
    lastEvent = now();
    if (e.type === "tool_execution_start") {
      const detail = String(e.args?.command ?? e.args?.path ?? "").replace(/\s+/g, " ").slice(0, 220);
      const label = `${e.toolName}${detail ? `: ${detail}` : ""}`;
      active.set(e.toolCallId, { label, started: now() });
      emit(`${role} ▶ ${label}`);
    } else if (e.type === "tool_execution_end") {
      const tool = active.get(e.toolCallId);
      emit(`${role} ${e.isError ? "FAILED" : "✓"} ${e.toolName} (${tool ? seconds(tool.started) : "unknown duration"})`);
      active.delete(e.toolCallId);
    } else if (e.type === "message_end" && e.message?.role === "assistant") {
      lastAssistant = e.message;
      for (const content of e.message.content ?? []) {
        if (content.type === "text") emit(`${role}: ${content.text.replace(/\s+/g, " ").slice(0, 600)}`);
      }
    } else if (e.type.includes("retry") || e.type.includes("compaction")) {
      emit(`${role}: ${e.type}`);
    }
  }
  return {
    push(chunk: Buffer) {
      pending += decoder.write(chunk);
      let newline: number;
      while ((newline = pending.indexOf("\n")) !== -1) {
        const line = pending.slice(0, newline);
        pending = pending.slice(newline + 1);
        if (line.trim()) event(JSON.parse(line));
      }
    },
    heartbeat() {
      const work = [...active.values()].map(t => `${t.label} (${seconds(t.started)})`).join("; ");
      emit(`${role} heartbeat: elapsed ${seconds(started)}, last event ${seconds(lastEvent)} ago; ${work || "waiting for model/API (no active tool)"}`);
    },
    finish() {
      pending += decoder.end();
      if (pending.trim()) event(JSON.parse(pending));
      return lastAssistant;
    },
  };
}
