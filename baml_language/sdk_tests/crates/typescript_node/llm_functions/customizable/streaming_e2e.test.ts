// Keyless streaming smokes — string-typed and class-typed `T`. The TypeScript
// sibling of python_pydantic2's `test_streaming_e2e.py`, matching its style:
// every test runs against an in-process BAML replay server (via the
// `withReplayServer` wrapper from `replay_harness.ts`) replaying a checked-in
// SSE recording, so the full bridge → BAML LLM client → HTTP → SSE →
// StreamAccumulator → SAP → Stream.next()/final() path is exercised with **no
// `OPENAI_API_KEY`**.
//
//   stream_e2e_extract(text)     -> string        ($stream: Stream<string | null, string>)
//   stream_e2e_extract_doc(text) -> StreamingDoc   ($stream: Stream<StreamingDoc$stream | null, StreamingDoc>)
//
// The recordings stream many SSE chunks, so each `next()` yields >= 10 partials
// before `StreamFinished` (asserted below); finals are checked for type, not
// exact content. The class-typed tests are the bridge-level regression guard for
// the class-typed streaming bug (bridge-generics/streaming, doc 00).
//
// Run: cargo nextest run -p sdk_test_typescript_node llm_functions::vitest
//
// Skipped on Windows: the replay server's SSE chunk pacing drains too fast
// there, so `next()` collapses many partials into one and the `>= 10 partials`
// asserts flake. Tracked in Linear B-315; remove the skip once that's fixed.

import { describe, it, expect } from "vitest";

// Importing the SDK root is REQUIRED before any function call: runtime
// initialization happens as a side effect of the root module.
import "./baml_sdk/index.js";
import * as lorem from "./baml_sdk/lorem/index.js";
import { StreamFinished } from "./baml_sdk/baml/stream/index.js";
import { withReplayServer } from "./replay_harness.js";

const T = 30_000;

// See header comment: streaming replay flakes on Windows pending investigation.
const SKIP_ON_WINDOWS = process.platform === "win32";

describe.skipIf(SKIP_ON_WINDOWS)("streaming e2e — string-typed T", () => {
  it(
    "next() yields >= 10 partials and drains to StreamFinished",
    { timeout: T },
    withReplayServer("replay_extract_string", () => {
      const stream = lorem.stream_e2e_extract$stream("ignored-by-replay-server");
      let results = 0;
      for (;;) {
        const v: unknown = stream.next();
        if (v instanceof StreamFinished) break;
        results += 1;
        expect(v === null || typeof v === "string").toBe(true);
        expect(results).toBeLessThan(10_000);
      }
      expect(results).toBeGreaterThanOrEqual(10);
      expect(typeof stream.final()).toBe("string");
    }),
  );

  it(
    "async: nextAsync() yields >= 10 partials",
    { timeout: T },
    withReplayServer("replay_extract_string", async () => {
      const stream = await lorem.stream_e2e_extract$stream_async("ignored-by-replay-server");
      let results = 0;
      for (;;) {
        const v: unknown = await stream.nextAsync();
        if (v instanceof StreamFinished) break;
        results += 1;
        expect(v === null || typeof v === "string").toBe(true);
        expect(results).toBeLessThan(10_000);
      }
      expect(results).toBeGreaterThanOrEqual(10);
      expect(typeof (await stream.finalAsync())).toBe("string");
    }),
  );

  it(
    "BAML-driven collect keeps the S | StreamFinished union engine-side",
    { timeout: T },
    withReplayServer("replay_extract_string", () => {
      const result = lorem.stream_e2e_collect("ignored-by-replay-server");
      expect(result).toBeInstanceOf(lorem.StreamE2ECollectResult);
      expect(result.next_calls.length).toBeGreaterThanOrEqual(10);
      for (const item of result.next_calls) {
        expect(item === null || typeof item === "string").toBe(true);
      }
      expect(typeof result.final_call).toBe("string");
    }),
  );
});

describe.skipIf(SKIP_ON_WINDOWS)("streaming e2e — class-typed T", () => {
  it(
    "next() yields >= 10 doc partials; final() is a typed StreamingDoc",
    { timeout: T },
    withReplayServer("replay_extract_doc", () => {
      const stream = lorem.stream_e2e_extract_doc$stream("ignored-by-replay-server");
      let results = 0;
      for (;;) {
        const v: unknown = stream.next();
        if (v instanceof StreamFinished) break;
        results += 1;
        if (v !== null) expect(v).toHaveProperty("title");
        expect(results).toBeLessThan(10_000);
      }
      expect(results).toBeGreaterThanOrEqual(10);
      expect(stream.final()).toBeInstanceOf(lorem.StreamingDoc);
    }),
  );

  it(
    "async: class-typed nextAsync() yields >= 10 partials",
    { timeout: T },
    withReplayServer("replay_extract_doc", async () => {
      const stream = await lorem.stream_e2e_extract_doc$stream_async("ignored-by-replay-server");
      let results = 0;
      for (;;) {
        const v: unknown = await stream.nextAsync();
        if (v instanceof StreamFinished) break;
        results += 1;
        if (v !== null) expect(v).toHaveProperty("title");
        expect(results).toBeLessThan(10_000);
      }
      expect(results).toBeGreaterThanOrEqual(10);
      expect(await stream.finalAsync()).toBeInstanceOf(lorem.StreamingDoc);
    }),
  );

  it(
    "BAML-driven collect returns the final doc",
    { timeout: T },
    withReplayServer("replay_extract_doc", () => {
      const result = lorem.stream_e2e_collect_doc("ignored-by-replay-server");
      expect(result).toBeInstanceOf(lorem.StreamingDoc);
      expect(result).toHaveProperty("title");
    }),
  );
});
