// Loads ../.env into process.env (no dotenv dep) and resolves the thinker
// endpoint: a real ANTHROPIC_API_KEY (env or .env) beats any proxy config.
// Import this FIRST from every entrypoint.

import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const envFile = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", ".env");
try {
  for (const line of fs.readFileSync(envFile, "utf8").split("\n")) {
    const m = line.match(/^([A-Z_][A-Z0-9_]*)=(.*)$/);
    if (m && process.env[m[1]] === undefined) {
      process.env[m[1]] = m[2].replace(/^['"]|['"]$/g, "");
    }
  }
} catch {}

// Direct Anthropic beats the claude-CLI proxy stand-in.
if (process.env.ANTHROPIC_API_KEY) {
  process.env.THINKER_API_KEY = process.env.ANTHROPIC_API_KEY;
  process.env.THINKER_BASE_URL = "https://api.anthropic.com";
  process.env.THINKER_MODEL ??= "claude-sonnet-4-6";
  console.error("[env] thinker → direct api.anthropic.com");
} else if (process.env.THINKER_BASE_URL?.includes("localhost")) {
  console.error(
    `[env] thinker → ${process.env.THINKER_BASE_URL} (claude-CLI proxy stand-in; ` +
      "add ANTHROPIC_API_KEY to server/.env for direct + faster)",
  );
}
