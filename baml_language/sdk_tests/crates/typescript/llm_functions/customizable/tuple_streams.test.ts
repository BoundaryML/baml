// Tuples as stream items, offline: `baml_sdk/tuple_streams` scripts each
// function's client to replay fixed text deltas, so the generated stream
// companions run the ordinary partial/final parse with no replay server (and
// no HTTP, so every runtime runs them). Web runtimes have no synchronous calls
// or stream pulls, so only Node also drives the sync forms.
//
//   stream_tuple_items$stream(text) -> Stream<(readonly [string, number])[]>
//     partials are lists of tuple arrays, `[["a", 1]]` first; the final value
//     is `[["a", 1], ["b", 2]]`.
//   stream_tuple_final$stream(text) -> Stream<readonly [number, string]>
//     every partial is itself a tuple array, and so is the final `[1, "a"]`.
import { describe, it, expect } from "vitest";

import "./baml_sdk/index.js";
import * as tuple_streams from "./baml_sdk/tuple_streams/index.js";
import { Done } from "./baml_sdk/ai/stream/index.js";
import { isTestRuntime } from "./test_runtime.js";

const hasSyncCalls = isTestRuntime("node");

type Pull<T> = () => T | Done;
type PullAsync<T> = () => Promise<T | Done>;

function drain<T>(next: Pull<T>): T[] {
  const partials: T[] = [];
  for (;;) {
    const v = next();
    if (v instanceof Done) return partials;
    partials.push(v);
    expect(partials.length).toBeLessThan(10_000);
  }
}

async function drainAsync<T>(next: PullAsync<T>): Promise<T[]> {
  const partials: T[] = [];
  for (;;) {
    const v = await next();
    if (v instanceof Done) return partials;
    partials.push(v);
    expect(partials.length).toBeLessThan(10_000);
  }
}

const ITEMS = [["a", 1], ["b", 2]];

function expectItemsPartials(partials: (readonly [string, number])[][]) {
  expect(partials.length).toBeGreaterThan(0);
  expect(partials[0]).toEqual([["a", 1]]);
  for (const partial of partials) {
    expect(Array.isArray(partial)).toBe(true);
    for (const item of partial) {
      // A tuple is an array of exactly its arity, slots in order.
      expect(Array.isArray(item)).toBe(true);
      expect(item).toHaveLength(2);
      expect(typeof item[0]).toBe("string");
      expect(typeof item[1]).toBe("number");
    }
    // Each partial is a prefix of the final value.
    expect(partial).toEqual(ITEMS.slice(0, partial.length));
  }
}

function expectFinalPartials(partials: (readonly [number, string])[]) {
  expect(partials.length).toBeGreaterThan(0);
  for (const partial of partials) {
    expect(Array.isArray(partial)).toBe(true);
    expect(partial).toHaveLength(2);
    expect(partial[0]).toBe(1);
    expect("a".startsWith(partial[1])).toBe(true);
  }
  expect(partials[partials.length - 1]).toEqual([1, "a"]);
}

describe("tuple streams", () => {
  // SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
  it("tuples_stream_items", async () => {
    if (hasSyncCalls) {
      const stream = tuple_streams.stream_tuple_items$stream("x");
      expectItemsPartials(drain(() => stream.next()));
      const final = stream.final();
      expect(final).toEqual(ITEMS);
      // The non-streaming call parses the same reply to the same value.
      expect(tuple_streams.stream_tuple_items("x")).toEqual(final);
    }

    const asyncStream = await tuple_streams.stream_tuple_items$stream_async("x");
    expectItemsPartials(await drainAsync(() => asyncStream.nextAsync()));
    expect(await asyncStream.finalAsync()).toEqual(ITEMS);
    expect(await tuple_streams.stream_tuple_items_async("x")).toEqual(ITEMS);
  });

  // SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
  it("tuples_stream_final", async () => {
    if (hasSyncCalls) {
      const stream = tuple_streams.stream_tuple_final$stream("x");
      expectFinalPartials(drain(() => stream.next()));
      const final = stream.final();
      expect(final).toEqual([1, "a"]);
      expect(tuple_streams.stream_tuple_final("x")).toEqual(final);
    }

    const asyncStream = await tuple_streams.stream_tuple_final$stream_async("x");
    expectFinalPartials(await drainAsync(() => asyncStream.nextAsync()));
    expect(await asyncStream.finalAsync()).toEqual([1, "a"]);
    expect(await tuple_streams.stream_tuple_final_async("x")).toEqual([1, "a"]);
  });
});
