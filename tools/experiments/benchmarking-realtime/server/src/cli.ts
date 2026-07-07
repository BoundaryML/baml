// Text-mode harness: full realtime session + BAML tool loop, no audio.
// Usage: OPENAI_API_KEY=... THINKER_MODEL=... THINKER_BASE_URL=... THINKER_API_KEY=... \
//        npm run text            (interactive REPL)
//        npm run text -- "one-shot question"

import "./env.js";
import * as readline from "node:readline";
import { RealtimeRelay } from "./relay.js";

const apiKey = process.env.OPENAI_API_KEY;
if (!apiKey) {
  console.error("OPENAI_API_KEY is required");
  process.exit(1);
}

const oneShot = process.argv.slice(2).join(" ").trim() || null;

const rl = oneShot
  ? null
  : readline.createInterface({ input: process.stdin, output: process.stdout });

let pendingText = "";

const relay = new RealtimeRelay({
  apiKey,
  outputModality: "text",
  events: {
    onReady: () => {
      console.log("[session ready]");
      if (oneShot) {
        console.log(`> ${oneShot}`);
        relay.sendText(oneShot);
      } else {
        prompt();
      }
    },
    onTextDelta: (t) => {
      pendingText += t;
      process.stdout.write(t);
    },
    onTextDone: () => {
      pendingText = "";
      process.stdout.write("\n");
      if (oneShot) {
        // Give the post-tool turn a chance to arrive before exiting: the
        // filler-phrase turn ends before the delegate round-trip completes.
        setTimeout(() => {
          relay.close();
          process.exit(0);
        }, 15000);
      } else {
        prompt();
      }
    },
    onToolResult: (outcome, args) => {
      console.log(
        `\n[delegate] request=${JSON.stringify(args.request)} -> tool=${outcome.tool} say=${JSON.stringify(outcome.say)}`,
      );
    },
    onError: (m) => console.error(`\n[error] ${m}`),
  },
});

function prompt() {
  rl?.question("> ", (line) => {
    const text = line.trim();
    if (!text) return prompt();
    if (text === "/quit") {
      relay.close();
      process.exit(0);
    }
    relay.sendText(text);
  });
}
