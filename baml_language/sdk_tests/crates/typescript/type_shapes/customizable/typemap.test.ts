import * as generatedSdk from "./baml_sdk/index.js";
import {
  BamlAudio,
  BamlImage,
  BamlPdf,
  BamlStream,
  BamlVideo,
  getTypeMap,
} from "@boundaryml/baml-bridge";
import { Sentiment } from "./baml_sdk/enums/index.js";
import { Wrapper, Wrapper$stream } from "./baml_sdk/generics/index.js";
import { Resume, Resume$stream } from "./baml_sdk/lorem/index.js";
import { Audio$stream, Image$stream, Pdf$stream, Video$stream } from "./baml_sdk/baml/media/index.js";
import { Stream$stream } from "./baml_sdk/baml/llm/index.js";
import { describe, expect, it } from "vitest";

describe("generated SDK typemap", () => {
  it("is installed during root-module evaluation", () => {
    expect(getTypeMap().getClass("user.lorem.Resume")).toBe(Resume);
  });

  it("resolves every runtime-owned base to one bridge constructor identity", () => {
    const cases = [
      ["baml.media.Image", generatedSdk.baml.media.Image, BamlImage],
      ["baml.media.Audio", generatedSdk.baml.media.Audio, BamlAudio],
      ["baml.media.Video", generatedSdk.baml.media.Video, BamlVideo],
      ["baml.media.Pdf", generatedSdk.baml.media.Pdf, BamlPdf],
      ["baml.llm.Stream", generatedSdk.baml.llm.Stream, BamlStream],
    ] as const;
    for (const [fqn, generatedConstructor, bridgeConstructor] of cases) {
      expect(generatedConstructor).toBe(bridgeConstructor);
      expect(getTypeMap().getClass(fqn)).toBe(bridgeConstructor);
      expect(getTypeMap().getClass(fqn)).toBe(generatedConstructor);
    }
  });

  it("preserves user, enum, generic, and companion mappings", () => {
    expect(getTypeMap().getClass("user.lorem.Resume")).toBe(Resume);
    expect(getTypeMap().jsTypeToBamlType(Resume)).toBe("user.lorem.Resume");
    expect(getTypeMap().getEnum("user.enums.Sentiment")).toBe(Sentiment);
    expect(getTypeMap().getClass("user.generics.Wrapper")).toBe(Wrapper);
    expect(getTypeMap().getClass("user.generics.Wrapper$stream")).toBe(Wrapper$stream);
    expect(getTypeMap().getClass("user.lorem.Resume$stream")).toBe(Resume$stream);
  });

  it("keeps generated stream companions distinct from runtime-owned bases", () => {
    for (const [fqn, companion, base] of [
      ["baml.media.Image$stream", Image$stream, BamlImage],
      ["baml.media.Audio$stream", Audio$stream, BamlAudio],
      ["baml.media.Video$stream", Video$stream, BamlVideo],
      ["baml.media.Pdf$stream", Pdf$stream, BamlPdf],
      ["baml.llm.Stream$stream", Stream$stream, BamlStream],
    ] as const) {
      expect(getTypeMap().getClass(fqn)).toBe(companion);
      expect(companion).not.toBe(base);
    }
  });
});
