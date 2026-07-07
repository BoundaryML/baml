// Side-by-side tool-calling comparison: "native" (standard realtime setup —
// JSON-schema tools in session.tools, the voice model extracts args) vs
// "delegate" (BAML thinker does selection + extraction). Both modes execute
// the SAME BAML executors, so scoring compares outcome.tool + outcome.data.
//
// Budget: ONE realtime session per mode; all cases run as turns in it.
// Usage: set -a && source .env && set +a && npx tsx src/compare.ts

import "./env.js";
import * as fs from "node:fs";
import { RealtimeRelay } from "./relay.js";
import { logTurn } from "./convexlog.js";
import type { ToolOutcome } from "../baml_sdk/index.js";

interface Case {
  id: string;
  utterance: string;
  /** expected outcome.tool; "none" = should answer without any tool */
  expectTool: string;
  /** validate the executor payload (outcome.data) */
  check?: (data: any) => boolean;
  expectNote: string;
}

const CASES: Case[] = [
  {
    id: "order-spelled",
    utterance: "check on my order a one b two dash c three d four",
    expectTool: "order",
    check: (d) => d?.order_id === "A1B2-C3D4",
    expectNote: "order_id A1B2-C3D4",
  },
  {
    id: "order-phonetic",
    utterance: "what's the status of order zee nine why eight dash ex seven double you six",
    expectTool: "order",
    check: (d) => d?.order_id === "Z9Y8-X7W6",
    expectNote: "order_id Z9Y8-X7W6",
  },
  {
    id: "timer-phrase",
    utterance: "set a timer for an hour and a half for the roast",
    expectTool: "timer",
    check: (d) => d?.seconds === 5400,
    expectNote: "5400 seconds",
  },
  {
    id: "timer-fraction",
    utterance: "give me a quarter hour timer",
    expectTool: "timer",
    check: (d) => d?.seconds === 900,
    expectNote: "900 seconds",
  },
  {
    id: "weather-disambig",
    utterance: "how's the weather in springfield, the one in illinois",
    expectTool: "weather",
    check: (d) => typeof d?.city === "string" && d.city.includes("Illinois"),
    expectNote: "Springfield, Illinois",
  },
  {
    id: "weather-followup",
    utterance: "and what about paris texas",
    expectTool: "weather",
    check: (d) => typeof d?.city === "string" && d.city.includes("Texas"),
    expectNote: "Paris, Texas",
  },
  {
    id: "no-tool",
    utterance: "what's the capital of mongolia",
    expectTool: "none",
    expectNote: "answer directly (no weather/order/timer tool)",
  },
];

const HARD_CASES: Case[] = [
  {
    id: "timer-third-hour",
    utterance: "set a timer for a third of an hour",
    expectTool: "timer",
    check: (d) => d?.seconds === 1200,
    expectNote: "1200 seconds",
  },
  {
    id: "order-confusables",
    utterance: "check order queue five are six dash ess seven tea eight",
    expectTool: "order",
    check: (d) => d?.order_id === "Q5R6-S7T8",
    expectNote: "order_id Q5R6-S7T8",
  },
  {
    id: "timer-fractional-min",
    utterance: "i need a timer for two and three quarter minutes",
    expectTool: "timer",
    check: (d) => d?.seconds === 165,
    expectNote: "165 seconds",
  },
  {
    id: "weather-indirect",
    utterance: "what's the weather where the golden gate bridge is",
    expectTool: "weather",
    check: (d) => typeof d?.city === "string" && d.city.includes("San Francisco"),
    expectNote: "San Francisco",
  },
  {
    id: "order-nato-mixed",
    utterance:
      "my order number is a as in apple one, b as in bravo two, then c three d four, with a dash in the middle",
    expectTool: "order",
    check: (d) => d?.order_id === "A1B2-C3D4",
    expectNote: "order_id A1B2-C3D4",
  },
];


// Nastier still: compound arithmetic, British/NATO letter names, inverted
// disambiguation, nicknames, world-knowledge durations, and one multi-intent
// (which native's parallel tool calls may legitimately win).
const ADV_CASES: Case[] = [
  {
    id: "timer-nested-math",
    utterance: "set a timer for half of half an hour",
    expectTool: "timer",
    check: (d) => d?.seconds === 900,
    expectNote: "900 seconds",
  },
  {
    id: "timer-arith",
    utterance: "timer for a fifth of an hour minus one minute",
    expectTool: "timer",
    check: (d) => d?.seconds === 660,
    expectNote: "660 seconds",
  },
  {
    id: "order-zed-british",
    utterance: "track order zed nine why eight hyphen ex seven double-u six",
    expectTool: "order",
    check: (d) => d?.order_id === "Z9Y8-X7W6",
    expectNote: "order_id Z9Y8-X7W6",
  },
  {
    id: "weather-inverted",
    utterance: "weather in the city with the eiffel tower, and i don't mean the vegas one",
    expectTool: "weather",
    check: (d) => typeof d?.city === "string" && d.city.includes("Paris") && !d.city.includes("Texas"),
    expectNote: "Paris, France (not Vegas/Texas)",
  },
  {
    id: "weather-nickname",
    utterance: "how's the big apple looking today",
    expectTool: "weather",
    check: (d) => typeof d?.city === "string" && d.city.includes("New York"),
    expectNote: "New York",
  },
  {
    id: "timer-world-knowledge",
    utterance: "give me a timer as long as one half of a soccer match",
    expectTool: "timer",
    check: (d) => d?.seconds === 2700,
    expectNote: "2700 seconds (45 min)",
  },
  {
    id: "multi-intent",
    utterance: "set a five minute tea timer and also tell me the weather in tokyo",
    expectTool: "timer",
    check: (d) => d?.seconds === 300,
    expectNote: "timer 300s (+ weather Tokyo is bonus)",
  },
];

interface ToolCallRecord {
  name: string;
  args: Record<string, any>;
  outcome: ToolOutcome;
}

interface CaseResult {
  id: string;
  toolCalls: ToolCallRecord[];
  finalText: string;
  ms: number;
  pass: boolean;
  detail: string;
}

function scoreCase(c: Case, calls: ToolCallRecord[], finalText: string): { pass: boolean; detail: string } {
  if (c.expectTool === "none") {
    // "answer" via delegate is fine; a real tool call is not.
    const realTools = calls.filter((t) => !["answer", "delegate", "none"].includes(t.outcome.tool));
    return realTools.length === 0
      ? { pass: true, detail: "no spurious tool call" }
      : { pass: false, detail: `unexpected tool: ${realTools.map((t) => t.outcome.tool).join(",")}` };
  }
  const hit = calls.find((t) => t.outcome.tool === c.expectTool);
  if (!hit) {
    return {
      pass: false,
      detail: `expected tool "${c.expectTool}", got [${calls.map((t) => t.outcome.tool).join(",") || "none"}]`,
    };
  }
  if (c.check && !c.check(hit.outcome.data)) {
    return { pass: false, detail: `wrong args: data=${JSON.stringify(hit.outcome.data)}` };
  }
  return { pass: true, detail: `ok: ${JSON.stringify(hit.outcome.data)}` };
}

async function runMode(mode: "native" | "delegate"): Promise<CaseResult[]> {
  console.log(`\n===== mode: ${mode} =====`);
  let calls: ToolCallRecord[] = [];
  let finalText = "";
  let settle: (() => void) | null = null;
  let ready: () => void = () => {};
  const readyPromise = new Promise<void>((res) => (ready = res));

  const relay = new RealtimeRelay({
    apiKey: process.env.OPENAI_API_KEY!,
    outputModality: "text",
    mode,
    events: {
      onReady: () => ready(),
      onTextDone: (t) => (finalText = t),
      onToolResult: (outcome, args, name) => calls.push({ name, args, outcome }),
      onSettled: () => settle?.(),
      onError: (m) => console.error(`  [error] ${m}`),
    },
  });
  await readyPromise;

  const results: CaseResult[] = [];
  for (const c of CASES_ACTIVE) {
    calls = [];
    finalText = "";
    const t0 = Date.now();
    const settled = new Promise<boolean>((res) => {
      const timer = setTimeout(() => res(false), 45000);
      settle = () => {
        clearTimeout(timer);
        res(true);
      };
    });
    relay.sendText(c.utterance);
    const ok = await settled;
    settle = null;
    const ms = Date.now() - t0;
    const { pass, detail } = ok
      ? scoreCase(c, calls, finalText)
      : { pass: false, detail: "TIMEOUT (45s)" };
    results.push({ id: c.id, toolCalls: calls, finalText, ms, pass, detail });
    logTurn({
      source: "compare",
      suite,
      caseId: c.id,
      agent: mode === "delegate" ? "baml" : "native",
      utterance: c.utterance,
      toolCalls: calls.map((t) => ({ name: t.name, args: t.args, tool: t.outcome.tool, data: t.outcome.data, say: t.outcome.say })),
      finalText,
      ms,
      pass,
      detail,
    });
    console.log(`  ${pass ? "PASS" : "FAIL"} ${c.id} (${ms}ms) — ${detail}`);
    console.log(`       reply: ${finalText.slice(0, 100)}`);
  }
  relay.close();
  return results;
}

// argv: [mode filter] [suite]. Suite "hard" switches to the hard cases and
// writes compare-results-hard.json. Mode filter spends only one session;
// results for the other mode are reloaded from the results file.
const argv = process.argv.slice(2);
const suite = argv.includes("adv") ? "adv" : argv.includes("hard") ? "hard" : "base";
const CASES_ACTIVE = suite === "adv" ? ADV_CASES : suite === "hard" ? HARD_CASES : CASES;
const RESULTS_FILE = `compare-results${suite === "base" ? "" : "-" + suite}.json`;
const only = (argv.find((a) => a === "native" || a === "delegate") ?? undefined) as
  | "native"
  | "delegate"
  | undefined;
let prior: { native?: CaseResult[]; delegate?: CaseResult[] } = {};
if (only) {
  try {
    prior = JSON.parse(fs.readFileSync(RESULTS_FILE, "utf8"));
  } catch {}
}
const native = only === "delegate" ? (prior.native ?? []) : await runMode("native");
const delegate = only === "native" ? (prior.delegate ?? []) : await runMode("delegate");

console.log("\n===== side by side =====");
console.log(
  "case".padEnd(20) + "expected".padEnd(28) + "native".padEnd(34) + "delegate (BAML)",
);
for (let i = 0; i < CASES_ACTIVE.length; i++) {
  const c = CASES_ACTIVE[i];
  const n = native[i];
  const d = delegate[i];
  const cell = (r: CaseResult) =>
    `${r.pass ? "✅" : "❌"} ${r.detail}`.slice(0, 32).padEnd(34);
  console.log(c.id.padEnd(20) + c.expectNote.padEnd(28) + cell(n) + `${d.pass ? "✅" : "❌"} ${d.detail}`.slice(0, 60));
}
const score = (rs: CaseResult[]) => `${rs.filter((r) => r.pass).length}/${rs.length}`;
const avg = (rs: CaseResult[]) => Math.round(rs.reduce((a, r) => a + r.ms, 0) / rs.length);
console.log(`\nnative:   ${score(native)} correct, avg ${avg(native)}ms/turn`);
console.log(`delegate: ${score(delegate)} correct, avg ${avg(delegate)}ms/turn`);

fs.writeFileSync(
  RESULTS_FILE,
  JSON.stringify({ when: new Date().toISOString(), native, delegate }, null, 2),
);
console.log(`\nfull transcript data → ${RESULTS_FILE}`);
process.exit(0);
