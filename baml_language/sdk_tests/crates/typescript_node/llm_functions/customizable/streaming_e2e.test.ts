// TypeScript sibling of python_pydantic2's `test_streaming_e2e.py` +
// `test_streaming_class_e2e.py`: end-to-end streaming against OpenAI for
// both string-typed and class-typed `T`.
//
//   StreamE2EExtract(text)       -> string        ($stream: Stream<string | null, string>)
//   StreamE2EExtractDoc(text)    -> StreamingDoc  ($stream: Stream<StreamingDoc$stream | null, StreamingDoc>)
//   StreamE2EExtractResume(text) -> Resume        ($stream: Stream<Resume$stream | null, Resume>)
//
// All use the `StreamStub` (openai) client. The runtime `Stream` object is
// the bridge's `BamlStream` (next/nextAsync/final/finalAsync); completion is
// signalled by a `baml.stream.StreamFinished` yield.
//
// Purpose: pin which parts of streaming work on the nodejs bridge.
// Current state (see thoughts/sam-projects/bridge-generics/streaming):
//
//   WORKS:  BAML-driven streaming (engine-side `next()` loop; only plain
//           values cross the FFI) — string- and class-typed alike. Final
//           class values decode to their generated classes via the typemap.
//   BROKEN: every host-driven `$stream` call (string- AND class-typed).
//           The companion returns the stream as an `ADT_TAGGED_HEAP_HANDLE`
//           (wire handle_type 14), and bridge_nodejs's `decodeValueHolder`
//           has no tagged-heap-handle arm (typescript_src/proto.ts — the
//           "TODO: ADT_TAGGED_HEAP_HANDLE" fallthrough), so the host gets a
//           bare `BamlHandle` with no `next()`/`final()` instead of
//           `BamlStream._fromHandle(...)`. bridge_python dispatches this arm
//           through the typemap (`baml.llm.Stream` → `BamlStream`,
//           baml_core/proto.py `_decode_handle`); Node needs the same.
//
// The broken cases are pinned with `it.fails` so this suite stays green
// today and flips loudly ("expected to fail, but passed") the moment the
// bridge arm is implemented — then drop the `.fails` markers.
//
// Skipped unless OPENAI_API_KEY is set. Run via:
//
//   infisical run -- cargo nextest run -p sdk_test_typescript_node llm_functions::vitest
//   (cd .../llm_functions/generated && infisical run -- pnpm exec vitest run streaming_e2e.test.ts)

import { describe, it, expect } from "vitest";

// Importing the SDK root is REQUIRED before any function call: runtime
// initialization (`initializeRuntimeFromBytecode` + `setTypeMap`) happens as
// a side effect of the root module, and ESM subpath imports do NOT execute
// the parent module (unlike Python, where importing `baml_sdk.lorem` runs
// the package __init__). Importing only `./baml_sdk/lorem/index.js` yields
// "BAML runtime has not been initialized" on the first call.
import "./baml_sdk/index.js";
import * as lorem from "./baml_sdk/lorem/index.js";
import { StreamFinished } from "./baml_sdk/baml/stream/index.js";

const RESUME =
  "Seasoned software engineer with 12 years of experience. Specializes " +
  "in Python and Rust. Currently based in Berlin. Interests include " +
  "distributed systems and developer tooling.";

const hasKey = !!process.env.OPENAI_API_KEY;

// Generous per-test timeout: each test is a live LLM round-trip.
const T = 120_000;

describe.skipIf(!hasKey)("streaming e2e — string-typed T", () => {
  it.fails("next() drives the stream to StreamFinished", { timeout: T }, () => {
    const stream = lorem.StreamE2EExtract$stream(RESUME);
    let iterations = 0;
    for (;;) {
      const v: unknown = stream.next();
      iterations += 1;
      if (v instanceof StreamFinished) break;
      expect(v === null || typeof v === "string").toBe(true);
      expect(iterations).toBeLessThan(10_000);
    }
    expect(iterations).toBeGreaterThanOrEqual(1);
  });

  it.fails("final() returns the complete string", { timeout: T }, () => {
    const stream = lorem.StreamE2EExtract$stream(RESUME);
    while (!((stream.next() as unknown) instanceof StreamFinished)) {
      /* drain */
    }
    const final = stream.final();
    expect(typeof final).toBe("string");
    expect(final.length).toBeGreaterThan(50);
  });

  it.fails("async: nextAsync()/finalAsync() round-trip", { timeout: T }, async () => {
    const stream = await lorem.StreamE2EExtract$stream_async(RESUME);
    let iterations = 0;
    for (;;) {
      const v: unknown = await stream.nextAsync();
      iterations += 1;
      if (v instanceof StreamFinished) break;
      expect(iterations).toBeLessThan(10_000);
    }
    const final = await stream.finalAsync();
    expect(typeof final).toBe("string");
    expect(final.length).toBeGreaterThan(50);
  });

  it("BAML-driven collect (StreamE2ECollect) returns the aggregate", { timeout: T }, () => {
    const result = lorem.StreamE2ECollect(RESUME);
    expect(result).toBeInstanceOf(lorem.StreamE2ECollectResult);
    expect(Array.isArray(result.next_calls)).toBe(true);
    expect(result.next_calls.length).toBeGreaterThanOrEqual(1);
    for (const item of result.next_calls) {
      expect(item === null || typeof item === "string").toBe(true);
    }
    expect(typeof result.final_call).toBe("string");
    expect(result.final_call.length).toBeGreaterThan(50);
  });
});

describe.skipIf(!hasKey)("streaming e2e — class-typed T", () => {
  it.fails("StreamingDoc (3 fields): next() reaches StreamFinished, partials are doc-shaped", { timeout: T }, () => {
    const stream = lorem.StreamE2EExtractDoc$stream(RESUME);
    let iterations = 0;
    let lastPartial: unknown = null;
    for (;;) {
      const v: unknown = stream.next();
      iterations += 1;
      if (v instanceof StreamFinished) break;
      if (v !== null) lastPartial = v;
      expect(iterations).toBeLessThan(10_000);
    }
    expect(iterations).toBeGreaterThanOrEqual(1);
    if (lastPartial !== null) {
      // Partials should decode as the `StreamingDoc$stream` companion.
      expect(lastPartial).toHaveProperty("title");
    }
  });

  it.fails("StreamingDoc (3 fields): final() returns a fully-typed doc", { timeout: T }, () => {
    const stream = lorem.StreamE2EExtractDoc$stream(RESUME);
    while (!((stream.next() as unknown) instanceof StreamFinished)) {
      /* drain */
    }
    const final = stream.final();
    expect(final).toBeInstanceOf(lorem.StreamingDoc);
    expect(typeof final.title).toBe("string");
    expect(typeof final.word_count).toBe("number");
  });

  it.fails("Resume (2 fields): final() returns a fully-typed resume", { timeout: T }, () => {
    const stream = lorem.StreamE2EExtractResume$stream(RESUME);
    while (!((stream.next() as unknown) instanceof StreamFinished)) {
      /* drain */
    }
    const final = stream.final();
    expect(final).toBeInstanceOf(lorem.Resume);
    expect(typeof final.name).toBe("string");
  });

  it.fails("async: class-typed nextAsync()/finalAsync() round-trip", { timeout: T }, async () => {
    const stream = await lorem.StreamE2EExtractDoc$stream_async(RESUME);
    let iterations = 0;
    for (;;) {
      const v: unknown = await stream.nextAsync();
      iterations += 1;
      if (v instanceof StreamFinished) break;
      expect(iterations).toBeLessThan(10_000);
    }
    const final = await stream.finalAsync();
    expect(final).toBeInstanceOf(lorem.StreamingDoc);
    expect(typeof final.title).toBe("string");
  });

  it("BAML-driven collect (StreamE2ECollectDoc) returns the final doc", { timeout: T }, () => {
    const result = lorem.StreamE2ECollectDoc(RESUME);
    expect(result).toBeInstanceOf(lorem.StreamingDoc);
    expect(typeof result.title).toBe("string");
  });
});
