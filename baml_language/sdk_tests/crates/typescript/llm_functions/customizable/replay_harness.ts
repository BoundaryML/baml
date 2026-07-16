// Keyless replay harness shared by the streaming tests — the TypeScript
// analogue of python_pydantic2's `replay_harness.py`.
//
// `withReplayServer(recording, body)` wraps a test so it runs against an
// in-process BAML server replaying a checked-in SSE recording, with the
// env-driven `StreamStub` client pointed at it — so the test exercises the full
// streaming path with **no `OPENAI_API_KEY`**. (Python applies this as an
// `@replay_server(...)` decorator; vitest has no test decorators, so we wrap the
// test body instead.)
import { existsSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { replay_serve_detached } from "./baml_sdk/replay/index.js";

// Absolute path to a checked-in SSE recording under sdk_tests/fixtures.
function recordingPath(name: string): string {
  let dir = dirname(fileURLToPath(import.meta.url));
  while (basename(dir) !== "sdk_tests") {
    const parent = dirname(dir);
    if (parent === dir) {
      throw new Error("could not locate the sdk_tests/ ancestor directory");
    }
    dir = parent;
  }
  const rec = join(
    dir,
    "fixtures",
    "llm_functions",
    "recordings",
    `${name}.snap.sse`,
  );
  if (!existsSync(rec)) throw new Error(`missing recording ${rec}`);
  return rec;
}

/**
 * Wrap a test to run against the keyless replay server for `recording`. Starts a
 * detached BAML replay server (serving the recording), points the env-driven
 * `StreamStub` client at it via `BAML_REPLAY_BASE_URL`, runs `body`, then shuts
 * the server down. Works for sync and async test bodies.
 */
export function withReplayServer(
  recording: string,
  body: () => void | Promise<void>,
): () => Promise<void> {
  return async () => {
    const addr = replay_serve_detached(recordingPath(recording));
    process.env.BAML_REPLAY_BASE_URL = `http://${addr}`;
    process.env.BAML_REPLAY_API_KEY = "replay-test-key";
    try {
      await body();
    } finally {
      try {
        await fetch(`http://${addr}/__replay__/shutdown`, { method: "POST" });
      } catch {
        // best-effort cooperative shutdown
      }
      delete process.env.BAML_REPLAY_BASE_URL;
      delete process.env.BAML_REPLAY_API_KEY;
    }
  };
}
