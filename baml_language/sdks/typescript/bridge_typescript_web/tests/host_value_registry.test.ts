import initWasm, * as raw from "#bridge-web-core";
import { beforeAll, describe, expect, it } from "vitest";

beforeAll(async () => {
  await initWasm();
});

describe("raw Web host-value registry", () => {
  it("uses one unique keyspace for callables and opaque values", () => {
    const before = raw._testWebHostCallableCount();
    const callableOne = raw.registerWebHostCallable(() => undefined);
    const opaque = raw.mintWebHostValueKey();
    const callableTwo = raw.registerWebHostCallable(() => undefined);

    expect(new Set([callableOne, opaque, callableTwo]).size).toBe(3);
    expect(raw._testWebHostCallableCount()).toBe(before + 2);
    raw.releaseWebHostCallable(callableOne);
    raw.releaseWebHostCallable(callableTwo);
    expect(raw._testWebHostCallableCount()).toBe(before);
  });

  it("installs the release callback once and keeps the first callback", () => {
    const first: bigint[] = [];
    const second: bigint[] = [];
    expect(raw._testWebHostReleaseCallbackInstalled()).toBe(false);
    expect(raw.registerWebHostValueReleaseCallback((key: bigint) => first.push(key))).toBe(true);
    expect(raw._testWebHostReleaseCallbackInstalled()).toBe(true);
    expect(raw.registerWebHostValueReleaseCallback((key: bigint) => second.push(key))).toBe(false);

    const key = raw.mintWebHostValueKey();
    raw._testWebFireHostRelease(key);
    expect(first).toEqual([key]);
    expect(second).toEqual([]);
  });

  it("rolls back callable registrations when a later value cannot encode", async () => {
    const { encodeCallArgs } = await import("@boundaryml/baml-bridge-web");
    const before = raw._testWebHostCallableCount();
    expect(() => encodeCallArgs(
      { value: [() => "registered first", Symbol("cannot encode")] },
      { callId: 1n },
    )).toThrow(/Cannot encode value/);
    expect(raw._testWebHostCallableCount()).toBe(before);
  });

  it("starts with no abandoned in-flight completions", () => {
    expect(raw._testWebInFlightHostCallCount()).toBe(0);
    expect(raw.completeWebHostCall(0xffff_ffff, 0, new Uint8Array())).toBe(false);
    expect(raw._testWebInFlightHostCallCount()).toBe(0);
  });

  it("classifies missing callable dispatch and removes its completion", async () => {
    const key = raw.mintWebHostValueKey();
    await expect(raw._testWebMissingHostCallableError(key)).resolves.toMatch(/not registered/);
    expect(raw._testWebInFlightHostCallCount()).toBe(0);
  });

  it("fails an allowed sync dispatch that does not complete reentrantly", { timeout: 2_000 }, () => {
    const key = raw.registerWebHostCallable(() => undefined);
    expect(raw._testWebSyncPendingHostCallableError(key)).toMatch(/did not complete reentrantly|async API/);
    expect(raw._testWebInFlightHostCallCount()).toBe(0);
    raw.releaseWebHostCallable(key);
  });
});
