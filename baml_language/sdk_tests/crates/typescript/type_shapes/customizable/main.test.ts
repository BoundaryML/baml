// Mirrors python_pydantic2/customizable/type_shapes/test_main.py.
//
// The bulk of type-shape verification happens via `tsc --noEmit` (the
// analog of pyright on the python side). These jest cases just
// confirm each generated namespace imports cleanly and that the
// representative symbols listed in 18a are reachable at runtime.

import { describe, it, expect } from "vitest";

import * as bamlSdk from "./baml_sdk/index.js";
import { Foo } from "./baml_sdk/index.js";
import { Resume } from "./baml_sdk/lorem/index.js";
import { Thing } from "./baml_sdk/a/b/index.js";
import * as primitives from "./baml_sdk/primitives/index.js";
import * as media from "./baml_sdk/media/index.js";
import * as enums from "./baml_sdk/enums/index.js";
import * as literals from "./baml_sdk/literals/index.js";
import * as classRefs from "./baml_sdk/class_refs/index.js";
import * as aliases from "./baml_sdk/aliases/index.js";
import * as aliasesConsumer from "./baml_sdk/aliases_consumer/index.js";
import * as optional from "./baml_sdk/optional/index.js";
import * as lists from "./baml_sdk/lists/index.js";
import * as maps from "./baml_sdk/maps/index.js";
import * as unions from "./baml_sdk/unions/index.js";
import * as recursion from "./baml_sdk/recursion/index.js";
import * as generics from "./baml_sdk/generics/index.js";
import * as forwardRefs from "./baml_sdk/forward_refs/index.js";
import * as complexModels from "./baml_sdk/complex_models/index.js";
import * as lorem from "./baml_sdk/lorem/index.js";
import * as a from "./baml_sdk/a/index.js";
import * as builtinMedia from "./baml_sdk/baml/media/index.js";
import * as builtinAiStream from "./baml_sdk/ai/stream/index.js";

describe("type_shapes — namespace imports", () => {
  it("main_root_imports_cleanly", () => {
    expect(bamlSdk).toBeDefined();
  });

  it("main_all_namespaces_reachable", () => {
    for (const mod of [
      primitives,
      media,
      enums,
      literals,
      classRefs,
      aliases,
      aliasesConsumer,
      optional,
      lists,
      maps,
      unions,
      recursion,
      generics,
      forwardRefs,
      complexModels,
      lorem,
      a,
      builtinMedia,
      builtinAiStream,
    ]) {
      expect(mod).toBeDefined();
    }
  });

  it("main_runtime_owned_builtin_leaves_expose_their_public_names", () => {
    for (const value of [builtinMedia.Image, builtinMedia.Audio, builtinMedia.Video, builtinMedia.Pdf, builtinAiStream.Stream]) {
      expect(value).toBeTypeOf("function");
    }
  });
});

describe("type_shapes — representative symbols", () => {
  it("main_root_foo_reachable", () => {
    expect(Foo).toBeDefined();
  });

  it("main_lorem_resume_reachable", () => {
    expect(Resume).toBeDefined();
  });

  it("main_deep_namespace_thing_reachable", () => {
    expect(Thing).toBeDefined();
  });
});
