// Keyless streaming smokes — string-typed and class-typed `T`. The TypeScript
// sibling of python_pydantic2's `test_streaming_e2e.py`, matching its style:
// every test runs against an in-process BAML replay server (via the
// `withReplayServer` wrapper from `replay_harness.ts`) replaying a checked-in
// SSE recording, so the full bridge → BAML LLM client → HTTP → SSE →
// StreamAccumulator → SAP → Stream.next()/final() path is exercised with **no
// `OPENAI_API_KEY`**.
//
//   stream_e2e_extract$stream(text)     -> Stream<string | null, string>
//   stream_e2e_extract_doc$stream(text) -> Stream<StreamingDoc$stream | null, StreamingDoc>
//
// The recordings stream many SSE chunks, so each `next()` yields >= 10 partials
// before `Done` (asserted below); finals are checked for type, not
// exact content. The class-typed tests are the bridge-level regression guard for
// the class-typed streaming bug (bridge-generics/streaming, doc 00).
//
// Run: cargo nextest run -p sdk_test_typescript llm_functions::vitest

import { describe, it, expect } from "vitest";

// Importing the SDK root is REQUIRED before any function call: runtime
// initialization happens as a side effect of the root module.
import "./baml_sdk/index.js";
import * as lorem from "./baml_sdk/lorem/index.js";
import { Done } from "./baml_sdk/ai/stream/index.js";
import { isTestRuntime } from "./test_runtime.js";

let withReplayServer: typeof import("./replay_harness.js").withReplayServer =
  () => async () => {};
if (isTestRuntime("node")) {
  ({ withReplayServer } = await import("./replay_harness.js"));
}

const T = 30_000;

// The replay harness owns a local node:http listener. Web HTTP streaming is deliberately unsupported; browser/workerd tagged-handle selection, cloning, and prompt failure live in bridge_typescript_web/tests/typemap_builtins.test.ts, while generated stream-companion shapes run everywhere in type_shapes/roundtrip_streams.test.ts.
describe.runIf(isTestRuntime("node"))("streaming e2e — string-typed T", () => {
  it(
    "streaming_e2e_next_yields_10_partials_and_drains_to_stream_finished",
    { timeout: T },
    withReplayServer("replay_extract_string", () => {
      const stream = lorem.stream_e2e_extract$stream(
        "ignored-by-replay-server",
      );
      let results = 0;
      for (;;) {
        const v: unknown = stream.next();
        if (v instanceof Done) break;
        results += 1;
        expect(v === null || typeof v === "string").toBe(true);
        expect(results).toBeLessThan(10_000);
      }
      expect(results).toBeGreaterThanOrEqual(10);
      expect(typeof stream.final()).toBe("string");
    }),
  );

  it(
    "streaming_e2e_async_next_async_yields_10_partials",
    { timeout: T },
    withReplayServer("replay_extract_string", async () => {
      const stream = await lorem.stream_e2e_extract$stream_async(
        "ignored-by-replay-server",
      );
      let results = 0;
      for (;;) {
        const v: unknown = await stream.nextAsync();
        if (v instanceof Done) break;
        results += 1;
        expect(v === null || typeof v === "string").toBe(true);
        expect(results).toBeLessThan(10_000);
      }
      expect(results).toBeGreaterThanOrEqual(10);
      expect(typeof (await stream.finalAsync())).toBe("string");
    }),
  );

  it(
    "streaming_e2e_baml_driven_collect_keeps_the_s_stream_finished_union_engine_side",
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

// The class replay cases use the same Node-only local HTTP provider harness.
describe.runIf(isTestRuntime("node"))("streaming e2e — class-typed T", () => {
  it(
    "streaming_e2e_next_yields_10_doc_partials_final_is_a_typed_streaming_doc",
    { timeout: T },
    withReplayServer("replay_extract_doc", () => {
      const stream = lorem.stream_e2e_extract_doc$stream(
        "ignored-by-replay-server",
      );
      let results = 0;
      for (;;) {
        const v: unknown = stream.next();
        if (v instanceof Done) break;
        results += 1;
        if (v !== null) expect(v).toHaveProperty("title");
        expect(results).toBeLessThan(10_000);
      }
      expect(results).toBeGreaterThanOrEqual(10);
      expect(stream.final()).toBeInstanceOf(lorem.StreamingDoc);
    }),
  );

  it(
    "streaming_e2e_async_class_typed_next_async_yields_10_partials",
    { timeout: T },
    withReplayServer("replay_extract_doc", async () => {
      const stream = await lorem.stream_e2e_extract_doc$stream_async(
        "ignored-by-replay-server",
      );
      let results = 0;
      for (;;) {
        const v: unknown = await stream.nextAsync();
        if (v instanceof Done) break;
        results += 1;
        if (v !== null) expect(v).toHaveProperty("title");
        expect(results).toBeLessThan(10_000);
      }
      expect(results).toBeGreaterThanOrEqual(10);
      expect(await stream.finalAsync()).toBeInstanceOf(lorem.StreamingDoc);
    }),
  );

  it(
    "streaming_e2e_baml_driven_collect_returns_the_final_doc",
    { timeout: T },
    withReplayServer("replay_extract_doc", () => {
      const result = lorem.stream_e2e_collect_doc("ignored-by-replay-server");
      expect(result).toBeInstanceOf(lorem.StreamingDoc);
      expect(result).toHaveProperty("title");
    }),
  );
});
