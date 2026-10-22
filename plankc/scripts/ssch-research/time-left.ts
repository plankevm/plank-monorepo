import { readFileSync } from "node:fs";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

// run.ts keeps the deadline in the attempt's state file and moves it on resume, so read it on every call.
export default function (pi: ExtensionAPI) {
  pi.registerTool({
    name: "time_left",
    label: "Time left",
    description: "Seconds left before this research attempt is stopped and fails.",
    parameters: { type: "object", properties: {} } as any,
    async execute() {
      const { deadline } = JSON.parse(readFileSync(process.env.SSCH_STATE!, "utf8"));
      const seconds = Math.max(0, Math.round((deadline - Date.now()) / 1000));
      return { content: [{ type: "text", text: `${seconds} seconds left` }], details: undefined };
    },
  });
}
