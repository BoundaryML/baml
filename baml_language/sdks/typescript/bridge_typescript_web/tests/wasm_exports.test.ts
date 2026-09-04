import initWasm, * as raw from "#bridge-web-core";
import { beforeAll, describe, expect, it } from "vitest";

const rawHandleMediaExports = [
  "cloneHandle",
  "mediaBase64",
  "mediaFile",
  "mediaFromBase64",
  "mediaFromFile",
  "mediaFromUrl",
  "mediaMimeType",
  "mediaUrl",
  "releaseHandle",
  "seedFunctionRefHandle",
  "seedGenericMediaHandle",
  "_testHandleTableEntryCount",
] as const;

beforeAll(async () => {
  await initWasm();
});

describe("raw WASM handle and media exports", () => {
  it("releases abandoned call reservations without reviving them on late cancellation", () => {
    const id = raw.newFunctionCall();
    expect(raw.cancelFunctionCall(id)).toBe(true);
    raw.releaseFunctionCall(id);
    expect(raw.cancelFunctionCall(id)).toBe(false);
    raw.releaseFunctionCall(id);
  });
  it("exposes the same named handle/media contract", () => {
    for (const name of rawHandleMediaExports) expect(raw[name]).toBeTypeOf("function");
  });

  it("keeps handle keys lossless and release idempotent", () => {
    const initialCount = raw._testHandleTableEntryCount();
    const original = raw.seedFunctionRefHandle(0xffff_ffff);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 1);
    const clone = raw.cloneHandle(original);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 2);

    expect(original).toBeTypeOf("bigint");
    expect(clone).toBeTypeOf("bigint");
    expect(clone).not.toBe(original);
    expect(raw.releaseHandle(original)).toBe(true);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 1);
    expect(raw.releaseHandle(original)).toBe(false);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount + 1);
    expect(raw.releaseHandle(clone)).toBe(true);
    expect(raw._testHandleTableEntryCount()).toBe(initialCount);
    expect(() => raw.cloneHandle(0n)).toThrow(/cloneHandle: invalid handle/);
  });

  it("constructs and inspects every media source form", () => {
    const url = raw.mediaFromUrl(1, "https://example.com/image.png", "image/png");
    expect(url).toBeTypeOf("bigint");
    expect(raw.mediaUrl(url, 6)).toBe("https://example.com/image.png");
    expect(raw.mediaFile(url, 6)).toBeUndefined();
    expect(raw.mediaBase64(url, 6)).toBe("");
    expect(raw.mediaMimeType(url, 6)).toBe("image/png");

    const file = raw.mediaFromFile(2, "/tmp/audio.wav");
    expect(raw.mediaFile(file, 7)).toBe("/tmp/audio.wav");
    expect(raw.mediaUrl(file, 7)).toBeUndefined();
    expect(raw.mediaMimeType(file, 7)).toBeUndefined();

    const base64 = raw.mediaFromBase64(4, "dm9pY2U=", "video/mp4");
    expect(raw.mediaBase64(base64, 8)).toBe("dm9pY2U=");
    expect(raw.mediaMimeType(base64, 8)).toBe("video/mp4");

    expect(raw.releaseHandle(url)).toBe(true);
    expect(raw.releaseHandle(file)).toBe(true);
    expect(raw.releaseHandle(base64)).toBe(true);
  });

  it("seeds generic media and validates explicit handle tags", () => {
    const generic = raw.seedGenericMediaHandle();
    expect(generic).toBeTypeOf("bigint");
    expect(raw.mediaUrl(generic, 10)).toBe("https://example.com/");
    expect(() => raw.mediaUrl(generic, 6)).toThrow(/mediaUrl: handle type mismatch/);
    expect(raw.releaseHandle(generic)).toBe(true);
  });
});
