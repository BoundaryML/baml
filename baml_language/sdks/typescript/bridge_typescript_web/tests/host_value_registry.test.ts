import initWasm, * as raw from "#bridge-web-core";
import { beforeAll, describe, expect, it } from "vitest";

beforeAll(async () => {
  await initWasm();
});

describe("raw Web host-value registry", () => {
  it("uses one unique keyspace for callables and opaque values", () => {
    const before = raw._testWebHostValueCount();
    const callableOne = raw.registerWebHostCallable(() => undefined);
    const opaque = raw.mintWebHostValueKey();
    const callableTwo = raw.registerWebHostCallable(() => undefined);

    expect(new Set([callableOne, opaque, callableTwo]).size).toBe(3);
    expect(raw._testWebHostValueCount()).toBe(before + 3);
    expect(raw.releaseWebHostValue(callableOne)).toBe(true);
    expect(raw.releaseWebHostValue(opaque)).toBe(true);
    expect(raw.releaseWebHostValue(callableTwo)).toBe(true);
    expect(raw._testWebHostValueCount()).toBe(before);
  });

  it("counts every send of a key and refuses one whose references are all given back", () => {
    const key = raw.registerWebHostCallable(() => undefined);
    raw.sendWebHostValueAgain(key);
    expect(raw.releaseWebHostValue(key)).toBe(false);
    expect(raw._testWebHostValueRegistered(key)).toBe(true);
    expect(raw.releaseWebHostValue(key)).toBe(true);
    expect(raw._testWebHostValueRegistered(key)).toBe(false);
    expect(() => raw.sendWebHostValueAgain(key)).toThrow(/no longer registered/);
    // A release for a key that is gone is ignored.
    expect(raw.releaseWebHostValue(key)).toBe(false);
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
    // Importing the runtime registers its host adapter with the bridge.
    await import("@boundaryml/baml-bridge-web");
    const before = raw._testWebHostValueCount();
    expect(() => raw._encodeWebInbound([() => "registered first", Symbol("cannot encode")], "value"))
      .toThrow(/cannot encode/);
    expect(raw._testWebHostValueCount()).toBe(before);
  });

  it("gives back only its own reference when a send of a held key is rolled back", async () => {
    const { BamlHandle } = await import("#bridge-web-native");
    const key = raw.registerWebHostCallable(() => undefined);
    const handle = new BamlHandle({ low: Number(BigInt.asIntN(32, key)), high: Number(BigInt.asIntN(32, key >> 32n)) }, 15);
    expect(() => raw._encodeWebInbound([handle, Symbol("cannot encode")], "value")).toThrow(/cannot encode/);
    expect(raw._testWebHostValueRegistered(key)).toBe(true);
    expect(raw.releaseWebHostValue(key)).toBe(true);
    expect(() => handle._cloneKeyForWire()).toThrow(/no longer registered/);
  });

  it("starts with no abandoned in-flight completions", () => {
    expect(raw._testWebInFlightHostCallCount()).toBe(0);
    expect(raw.completeWebHostCallBridgeFailure(0xffff_ffff, "no such call")).toBe(false);
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
    raw.releaseWebHostValue(key);
  });
});
