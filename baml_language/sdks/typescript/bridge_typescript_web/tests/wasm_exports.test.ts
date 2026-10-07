import initWasm, * as raw from "#bridge-web-core";
import { beforeAll, describe, expect, it } from "vitest";

const rawHandleMediaExports = [
  "cloneHandle",
  "mediaBase64",
  "mediaFromBase64",
  "mediaFromFileBytes",
  "mediaFromFileContent",
  "mediaFromUrl",
  "mediaMimeType",
  "mediaName",
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
    expect(raw.mediaName(url, 6)).toBeUndefined();
    expect(raw.mediaBase64(url, 6)).toBe("");
    expect(raw.mediaMimeType(url, 6)).toBe("image/png");

    // This module has no file system: the host reads, and passes bytes.
    const file = raw.mediaFromFileBytes(2, "/recordings/audio.wav", new TextEncoder().encode("voice"));
    expect(raw.mediaName(file, 7)).toBe("audio.wav");
    expect(raw.mediaBase64(file, 7)).toBe("dm9pY2U=");
    expect(raw.mediaUrl(file, 7)).toBeUndefined();
    expect(raw.mediaMimeType(file, 7)).toBe("audio/wav");

    const content = raw.mediaFromFileContent(3, "/reports/q3.pdf", "JVBERi0xLjc=");
    expect(raw.mediaName(content, 9)).toBe("q3.pdf");
    expect(raw.mediaMimeType(content, 9)).toBe("application/pdf");

    const base64 = raw.mediaFromBase64(4, "dm9pY2U=", "video/mp4");
    expect(raw.mediaBase64(base64, 8)).toBe("dm9pY2U=");
    expect(raw.mediaMimeType(base64, 8)).toBe("video/mp4");

    expect(raw.releaseHandle(url)).toBe(true);
    expect(raw.releaseHandle(file)).toBe(true);
    expect(raw.releaseHandle(content)).toBe(true);
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
