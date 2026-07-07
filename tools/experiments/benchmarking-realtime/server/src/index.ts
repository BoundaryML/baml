// Arena server: one browser connection drives TWO realtime sessions with the
// same input — "baml" (delegate mode, speaks aloud) and "native" (standard
// session.tools, text only) — so their tool calls can be compared live.
// Sessions are opened on an explicit client "start" message (2 budget units),
// never on page load.

import "./env.js";
import * as http from "node:http";
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { WebSocketServer } from "ws";
import { RealtimeRelay } from "./relay.js";
import { API_CALL_LIMIT, remainingApiCalls } from "./budget.js";
import { logTurn } from "./convexlog.js";

const apiKey = process.env.OPENAI_API_KEY;
if (!apiKey) {
  console.error("OPENAI_API_KEY is required");
  process.exit(1);
}

const PORT = Number(process.env.PORT ?? 8787);
const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "..");

function benchmarkResults() {
  const suites: Record<string, any> = {};
  for (const [suite, file] of [
    ["base", "compare-results.json"],
    ["hard", "compare-results-hard.json"],
    ["adversarial", "compare-results-adv.json"],
  ] as const) {
    try {
      suites[suite] = JSON.parse(fs.readFileSync(path.join(root, file), "utf8"));
    } catch {}
  }
  return suites;
}

const server = http.createServer((req, res) => {
  if (req.url === "/" || req.url === "/index.html") {
    res.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    res.end(fs.readFileSync(path.join(root, "public", "index.html")));
  } else if (req.url === "/results") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(benchmarkResults()));
  } else if (req.url === "/budget") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ remaining: remainingApiCalls(), limit: API_CALL_LIMIT }));
  } else {
    res.writeHead(404);
    res.end("not found");
  }
});

const wss = new WebSocketServer({ server, path: "/ws" });

wss.on("connection", (client) => {
  console.log("[client connected]");
  const push = (msg: Record<string, any>) => {
    if (client.readyState === client.OPEN) client.send(JSON.stringify(msg));
  };

  const relays: Partial<Record<"baml" | "native", RealtimeRelay>> = {};
  // Per-agent turn start (text send or VAD speech end) for latency readouts.
  const t0: Record<string, number> = {};
  // Per-agent turn accumulators for run logging.
  let utterance = "[voice turn]";
  const acc: Record<string, { toolCalls: any[]; finalText: string }> = {
    baml: { toolCalls: [], finalText: "" },
    native: { toolCalls: [], finalText: "" },
  };

  function makeRelay(agent: "baml" | "native"): RealtimeRelay {
    return new RealtimeRelay({
      apiKey: apiKey!,
      outputModality: agent === "baml" ? "audio" : "text",
      mode: agent === "baml" ? "delegate" : "native",
      events: {
        onReady: () => push({ type: "status", agent, data: "ready" }),
        onAudio: agent === "baml" ? (b64) => push({ type: "audio", data: b64 }) : undefined,
        onSpeechStopped: () => {
          t0[agent] = Date.now();
          utterance = "[voice turn]";
          push({ type: "speech_stopped", agent });
        },
        onTextDelta: (t) => push({ type: "text_delta", agent, data: t }),
        onTextDone: (t) => {
          acc[agent].finalText = t;
          push({ type: "text_done", agent, data: t });
        },
        onToolResult: (outcome, args, name) => {
          acc[agent].toolCalls.push({ name, args, tool: outcome.tool, data: outcome.data, say: outcome.say });
          push({
            type: "tool",
            agent,
            name,
            args,
            outcome,
            ms: t0[agent] ? Date.now() - t0[agent] : null,
          });
        },
        onSettled: () => {
          const ms = t0[agent] ? Date.now() - t0[agent] : null;
          if (acc[agent].toolCalls.length || acc[agent].finalText) {
            logTurn({
              source: "arena",
              agent,
              utterance,
              toolCalls: acc[agent].toolCalls,
              finalText: acc[agent].finalText,
              ms,
            });
            acc[agent] = { toolCalls: [], finalText: "" };
          }
          push({ type: "settled", agent, ms });
        },
        onError: (m) => {
          console.error(`[${agent} error]`, m);
          push({ type: "error", agent, data: m });
        },
      },
    });
  }

  client.on("message", (raw) => {
    let msg: Record<string, any>;
    try {
      msg = JSON.parse(raw.toString());
    } catch {
      return;
    }
    switch (msg.type) {
      case "start":
        if (relays.baml) break; // already started
        if (remainingApiCalls() < 2) {
          push({
            type: "error",
            agent: "system",
            data: `API budget exhausted (${API_CALL_LIMIT} sessions). Raise API_CALL_LIMIT in .env or delete .api-budget.json.`,
          });
          break;
        }
        relays.baml = makeRelay("baml");
        relays.native = makeRelay("native");
        push({ type: "budget", remaining: remainingApiCalls(), limit: API_CALL_LIMIT });
        break;
      case "audio":
        if (typeof msg.data === "string") {
          relays.baml?.appendAudio(msg.data);
          relays.native?.appendAudio(msg.data);
        }
        break;
      case "text":
        if (typeof msg.data === "string") {
          t0.baml = t0.native = Date.now();
          utterance = msg.data;
          relays.baml?.sendText(msg.data);
          relays.native?.sendText(msg.data);
        }
        break;
    }
  });

  client.on("close", () => {
    console.log("[client disconnected]");
    relays.baml?.close();
    relays.native?.close();
  });
});

server.listen(PORT, () => {
  console.log(`arena at http://localhost:${PORT} — budget ${remainingApiCalls()}/${API_CALL_LIMIT} sessions left`);
});
