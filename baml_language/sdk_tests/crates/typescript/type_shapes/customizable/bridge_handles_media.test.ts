import {
  BamlAudio,
  BamlHandle,
  BamlImage,
  BamlPdf,
  BamlVideo,
  _seedFunctionRefHandle,
  _seedGenericMediaHandle,
} from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import { isTestRuntime } from "./test_runtime.js";

type Releasable = { _releaseForTest(): boolean };

const isWebRuntime = isTestRuntime("web") || isTestRuntime("workers");

function releaseForWeb(value: unknown): boolean {
  return (value as Releasable)._releaseForTest();
}

describe("bridge handles and media — key ownership", () => {
  it("bridge_handles_media_normalizes_key_halves_losslessly_and_returns_defensive_key_objects", () => {
    const input = { low: 0xffff_ffff, high: -1 };
    const handle = new BamlHandle(input, 15);
    input.low = 0;

    const first = handle.key;
    expect(first).toEqual({ low: -1, high: -1 });
    first.high = 0;
    expect(handle.key).toEqual({ low: -1, high: -1 });
  });

  it("bridge_handles_media_clones_ordinary_owners_and_wire_keys_independently", () => {
    const [key, handleType] = _seedFunctionRefHandle(0xffff_ffff);
    expect(handleType).toBe(5);
    const owner = new BamlHandle(key, handleType);
    const clone = owner.clone();
    const wireKeyOne = owner._cloneKeyForWire();
    const wireKeyTwo = owner._cloneKeyForWire();

    expect(clone.key).not.toEqual(owner.key);
    expect(wireKeyOne).not.toEqual(owner.key);
    expect(wireKeyTwo).not.toEqual(owner.key);
    expect(wireKeyTwo).not.toEqual(wireKeyOne);
    expect(owner.clone()).toBeInstanceOf(BamlHandle);
    expect(clone.clone()).toBeInstanceOf(BamlHandle);

    if (isWebRuntime) {
      expect(releaseForWeb(new BamlHandle(wireKeyOne, handleType))).toBe(true);
      expect(releaseForWeb(new BamlHandle(wireKeyTwo, handleType))).toBe(true);
      expect(releaseForWeb(owner)).toBe(true);
      expect(releaseForWeb(owner)).toBe(false);
      expect(() => owner.clone()).toThrow(/released/);
      expect(clone.clone()).toBeInstanceOf(BamlHandle);
      expect(releaseForWeb(clone)).toBe(true);
    }
  });

  it("bridge_handles_media_rejects_an_invalid_ordinary_key_only_when_an_operation_resolves_it", () => {
    const invalid = new BamlHandle({ low: 0x7fff_fffe, high: 0 }, 5);
    expect(invalid.key).toEqual({ low: 0x7fff_fffe, high: 0 });
    expect(() => invalid.clone()).toThrow(/invalid handle/);
    if (isWebRuntime) expect(releaseForWeb(invalid)).toBe(false);
  });

  it("bridge_handles_media_keeps_host_value_tags_outside_ordinary_clone_and_release", () => {
    if (!isWebRuntime) return;
    for (const handleType of [15, 16]) {
      const owner = new BamlHandle({ low: 1, high: 0 }, handleType);
      expect(owner.clone().key).toEqual(owner.key);
      expect(owner._cloneKeyForWire()).toEqual(owner.key);
      expect(releaseForWeb(owner)).toBe(false);
      expect(owner.clone()).toBeInstanceOf(BamlHandle);
    }
  });

  it("bridge_handles_media_exposes_stable_seed_tags", () => {
    const [functionKey, functionType] = _seedFunctionRefHandle(7);
    const [mediaKey, mediaType] = _seedGenericMediaHandle();
    expect(functionType).toBe(5);
    expect(mediaType).toBe(10);
    if (isWebRuntime) {
      expect(releaseForWeb(new BamlHandle(functionKey, functionType))).toBe(true);
      expect(releaseForWeb(new BamlHandle(mediaKey, mediaType))).toBe(true);
    }
  });
});

const mediaKinds = [
  ["BamlImage", BamlImage, 6],
  ["BamlAudio", BamlAudio, 7],
  ["BamlVideo", BamlVideo, 8],
  ["BamlPdf", BamlPdf, 9],
] as const;

type MediaValue = {
  url(): string | null;
  file(): string | null;
  base64(): string;
  mimeType(): string | null;
  _toHandle(): BamlHandle;
};

type MediaConstructor = {
  fromUrl(url: string, mimeType?: string | null): MediaValue;
  fromFile(file: string, mimeType?: string | null): MediaValue;
  fromBase64(base64: string, mimeType?: string | null): MediaValue;
  _fromHandle(handle: BamlHandle): MediaValue;
};

describe.each(mediaKinds)(
  "bridge handles and media — %s",
  (
    _name: string,
    Media: MediaConstructor,
    handleType: number,
  ) => {
  // SDK_PARITY_LINT(skip): exercises TypeScript bridge media descriptor APIs
  it("bridge_handles_media_constructs_url_file_and_base64_descriptors", () => {
    const url = Media.fromUrl("https://example.com/asset", "application/test");
    expect(url.url()).toBe("https://example.com/asset");
    expect(url.file()).toBeNull();
    expect(url.base64()).toBe("");
    expect(url.mimeType()).toBe("application/test");

    const file = Media.fromFile("/tmp/asset");
    expect(file.url()).toBeNull();
    expect(file.file()).toBe("/tmp/asset");
    expect(file.mimeType()).toBeNull();

    const base64 = Media.fromBase64("aGVsbG8=", "application/octet-stream");
    expect(base64.base64()).toBe("aGVsbG8=");
    expect(base64.mimeType()).toBe("application/octet-stream");

    if (isWebRuntime) {
      expect(releaseForWeb(url)).toBe(true);
      expect(releaseForWeb(file)).toBe(true);
      expect(releaseForWeb(base64)).toBe(true);
      expect(() => url.url()).toThrow(/released/);
    }
  });

  // SDK_PARITY_LINT(skip): exercises TypeScript bridge media handle ownership APIs
  it("bridge_handles_media_clones_ownership_through_to_handle_and_from_handle", () => {
    const original = Media.fromUrl("https://example.com/asset", "application/test");
    const handle = original._toHandle();
    expect(handle.handleType).toBe(handleType);
    const decoded = Media._fromHandle(handle);
    expect(decoded.url()).toBe("https://example.com/asset");
    expect(decoded.mimeType()).toBe("application/test");
    expect(original._toHandle().key).not.toEqual(handle.key);

    const wrongMedia = handleType === 6 ? BamlAudio : BamlImage;
    expect(() => wrongMedia._fromHandle(handle)).toThrow();

    if (isWebRuntime) {
      expect(releaseForWeb(handle)).toBe(true);
      expect(releaseForWeb(original)).toBe(true);
      expect(decoded.url()).toBe("https://example.com/asset");
      expect(releaseForWeb(decoded)).toBe(true);
    }
  });
  },
);
