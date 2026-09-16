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
import { Wrapper } from "./baml_sdk/generics/index.js";
import { Resume } from "./baml_sdk/lorem/index.js";
import { describe, expect, it } from "vitest";

describe("generated SDK typemap", () => {
  it("typemap_is_installed_during_root_module_evaluation", () => {
    expect(getTypeMap().getClass("user.lorem.Resume")).toBe(Resume);
  });

  it("typemap_resolves_every_runtime_owned_base_to_one_bridge_constructor_identity", () => {
    const cases = [
      ["baml.media.Image", generatedSdk.baml.media.Image, BamlImage],
      ["baml.media.Audio", generatedSdk.baml.media.Audio, BamlAudio],
      ["baml.media.Video", generatedSdk.baml.media.Video, BamlVideo],
      ["baml.media.Pdf", generatedSdk.baml.media.Pdf, BamlPdf],
      ["ai.stream.Stream", generatedSdk.ai.stream.Stream, BamlStream],
    ] as const;
    for (const [fqn, generatedConstructor, bridgeConstructor] of cases) {
      expect(generatedConstructor).toBe(bridgeConstructor);
      expect(getTypeMap().getClass(fqn)).toBe(bridgeConstructor);
      expect(getTypeMap().getClass(fqn)).toBe(generatedConstructor);
    }
  });

  it("typemap_preserves_user_enum_and_generic_mappings", () => {
    expect(getTypeMap().getClass("user.lorem.Resume")).toBe(Resume);
    expect(getTypeMap().jsTypeToBamlType(Resume)).toBe("user.lorem.Resume");
    expect(getTypeMap().getEnum("user.enums.Sentiment")).toBe(Sentiment);
    expect(getTypeMap().getClass("user.generics.Wrapper")).toBe(Wrapper);
  });
});
