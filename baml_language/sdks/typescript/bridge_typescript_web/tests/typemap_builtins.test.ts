import initWasm, * as raw from "#bridge-web-core";
import { baml_bridge } from "../dist/shared/proto/baml_cffi.js";
import {
  BamlImage,
  BamlStream,
  BamlTypeMap,
  decodeCallResult,
  setTypeMap,
} from "@boundaryml/baml-bridge-web";
import { beforeAll, describe, expect, it } from "vitest";

const { BamlOutboundResult, InboundValue } = baml_bridge.cffi.v1;

function keyHalves(key: bigint): { low: number; high: number } {
  const normalized = BigInt.asUintN(64, key);
  return {
    low: Number(BigInt.asIntN(32, normalized)),
    high: Number(BigInt.asIntN(32, normalized >> 32n)),
  };
}

function decode(result: baml_bridge.cffi.v1.IBamlOutboundResult): unknown {
  return decodeCallResult(Uint8Array.from(BamlOutboundResult.encode(BamlOutboundResult.create(result)).finish()));
}

function bigintFromWireKey(key: number | { low: number; high: number }): bigint {
  if (typeof key === "number") return BigInt(key);
  return (BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0);
}

beforeAll(async () => {
  await initWasm();
  setTypeMap(BamlTypeMap.fromLazyEntries({
    classes: {
      "ai.stream.Stream": () => BamlStream,
      "test.stream.Custom": () => BamlStream,
      "baml.media.Image": () => BamlImage,
    },
    enums: {},
    typeAliases: {},
  }));
});

describe("builtin typemap decoding", () => {
  it("converges class-envelope and typed-handle media on BamlImage", () => {
    const classKey = raw.mediaFromUrl(1, "https://example.test/class.png", "image/png");
    const classValue = decode({
      ok: {
        classValue: {
          name: "baml.media.Image",
          fields: [{
            key: "_data",
            value: { handleValue: { key: keyHalves(classKey), handleType: 6 } },
          }],
        },
      },
    });
    expect(classValue).toBeInstanceOf(BamlImage);
    expect((classValue as BamlImage).url()).toBe("https://example.test/class.png");

    const handleKey = raw.mediaFromUrl(1, "https://example.test/handle.png", "image/png");
    const handleValue = decode({ ok: { handleValue: { key: keyHalves(handleKey), handleType: 6 } } });
    expect(handleValue).toBeInstanceOf(BamlImage);
    expect((handleValue as BamlImage).constructor).toBe((classValue as BamlImage).constructor);
    expect((handleValue as BamlImage).url()).toBe("https://example.test/handle.png");
  });

  it("decodes a tagged handle as BamlStream and clones ownership when encoding", () => {
    const initialCount = raw._testHandleTableEntryCount();
    const originalKey = raw.seedGenericMediaHandle();
    const stream = decode({
      ok: {
        handleValue: {
          key: keyHalves(originalKey),
          handleType: 14,
          ty: { classTy: { name: "ai.stream.Stream" } },
        },
      },
    });

    expect(stream).toBeInstanceOf(BamlStream);
    expect((stream as unknown as { _classFqn: string })._classFqn).toBe("ai.stream.Stream");
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 1);
    const bytes = raw._encodeWebInbound(stream, "self");
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 2);

    const wireHandle = InboundValue.decode(bytes).handle;
    if (wireHandle === undefined || wireHandle === null) throw new Error("stream did not encode as a handle");
    expect(wireHandle.handleType).toBe(14);
    expect(raw.releaseHandle(bigintFromWireKey(wireHandle.key))).toBe(true);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 1);

    const clonedHandle = (stream as BamlStream<unknown>)._toHandle().clone();
    const clonedStream = BamlStream._fromHandle(clonedHandle, "ai.stream.Stream");
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 2);
    expect((stream as BamlStream<unknown>)._toHandle()._releaseForTest()).toBe(true);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 1);

    const cloneBytes = raw._encodeWebInbound(clonedStream, "self");
    const cloneWireHandle = InboundValue.decode(cloneBytes).handle;
    if (cloneWireHandle === undefined || cloneWireHandle === null) throw new Error("cloned stream did not encode as a handle");
    expect(raw.releaseHandle(bigintFromWireKey(cloneWireHandle.key))).toBe(true);
    expect(clonedHandle._releaseForTest()).toBe(true);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount);
  });

  it("preserves the tagged handle class FQN on BamlStream", () => {
    const initialCount = raw._testHandleTableEntryCount();
    const originalKey = raw.seedGenericMediaHandle();
    const stream = decode({
      ok: {
        handleValue: {
          key: keyHalves(originalKey),
          handleType: 14,
          ty: { classTy: { name: "test.stream.Custom" } },
        },
      },
    }) as BamlStream<unknown>;

    expect((stream as unknown as { _classFqn: string })._classFqn).toBe("test.stream.Custom");
    expect(stream._toHandle()._releaseForTest()).toBe(true);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount);
  });

  // Real stateful nextAsync/finalAsync success requires the deliberately
  // unsupported Web HTTP-streaming SysOp. The Node replay fixture covers that
  // provider path, and web_sysops.test.ts proves async stream creation rejects.
  // This synthetic handle specifically guards the sync wrapper from entering
  // WASM and deadlocking the JavaScript event loop.
  it("fails synthetic sync stream pulls before entering WASM", { timeout: 2_000 }, () => {
    const initialCount = raw._testHandleTableEntryCount();
    const originalKey = raw.seedGenericMediaHandle();
    const stream = decode({
      ok: {
        handleValue: {
          key: keyHalves(originalKey),
          handleType: 14,
          ty: { classTy: { name: "ai.stream.Stream" } },
        },
      },
    }) as BamlStream<unknown>;

    expect(() => stream.next()).toThrow(/nextAsync|finalAsync/);
    expect(() => stream.final()).toThrow(/nextAsync|finalAsync/);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 1);
    expect(stream._toHandle()._releaseForTest()).toBe(true);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount);
  });
});
