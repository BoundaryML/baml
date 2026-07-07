// Hard budget on real OpenAI realtime API sessions. Persists across process
// runs in .api-budget.json next to package.json. When the limit is hit, the
// process exits immediately — no session is opened.

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const BUDGET_FILE = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  ".api-budget.json",
);

export const API_CALL_LIMIT = Number(process.env.API_CALL_LIMIT ?? 10);

function readCount(): number {
  try {
    return JSON.parse(fs.readFileSync(BUDGET_FILE, "utf8")).count ?? 0;
  } catch {
    return 0;
  }
}

/** Call before opening a real realtime session. Exits the process if spent. */
export function spendApiCall(label: string): void {
  const count = readCount();
  if (count >= API_CALL_LIMIT) {
    console.error(
      `[budget] KILL SWITCH: ${count}/${API_CALL_LIMIT} realtime API sessions already used. ` +
        `Refusing to open another (wanted: ${label}). ` +
        `Reset by deleting ${BUDGET_FILE} or raise API_CALL_LIMIT.`,
    );
    process.exit(2);
  }
  fs.writeFileSync(BUDGET_FILE, JSON.stringify({ count: count + 1 }));
  console.error(`[budget] realtime session ${count + 1}/${API_CALL_LIMIT} (${label})`);
}

export function remainingApiCalls(): number {
  return Math.max(0, API_CALL_LIMIT - readCount());
}
