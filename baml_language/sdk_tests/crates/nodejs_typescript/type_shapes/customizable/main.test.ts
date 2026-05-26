// Mirrors python_pydantic2/customizable/type_shapes/test_main.py.
//
// The bulk of type-shape verification happens via `tsc --noEmit` (the
// analog of pyright on the python side). These jest cases just
// confirm each generated namespace imports cleanly and that the
// representative symbols listed in 18a are reachable at runtime.

import { describe, it, expect } from "@jest/globals";

import * as bamlSdk from "./baml_sdk";
import { Foo } from "./baml_sdk";
import { Resume } from "./baml_sdk/lorem";
import { Thing } from "./baml_sdk/a/b";
import * as primitives from "./baml_sdk/primitives";
import * as media from "./baml_sdk/media";
import * as enums from "./baml_sdk/enums";
import * as literals from "./baml_sdk/literals";
import * as classRefs from "./baml_sdk/class_refs";
import * as aliases from "./baml_sdk/aliases";
import * as aliasesConsumer from "./baml_sdk/aliases_consumer";
import * as optional from "./baml_sdk/optional";
import * as lists from "./baml_sdk/lists";
import * as maps from "./baml_sdk/maps";
import * as unions from "./baml_sdk/unions";
import * as recursion from "./baml_sdk/recursion";
import * as generics from "./baml_sdk/generics";
import * as forwardRefs from "./baml_sdk/forward_refs";
import * as lorem from "./baml_sdk/lorem";
import * as a from "./baml_sdk/a";

describe("type_shapes — namespace imports", () => {
  it("baml_sdk root imports cleanly", () => {
    expect(bamlSdk).toBeDefined();
  });

  it("every namespace module imports cleanly", () => {
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
      lorem,
      a,
    ]) {
      expect(mod).toBeDefined();
    }
  });
});

describe("type_shapes — representative symbols", () => {
  it("root Foo is reachable", () => {
    expect(Foo).toBeDefined();
  });

  it("baml_sdk/lorem.Resume is reachable", () => {
    expect(Resume).toBeDefined();
  });

  it("baml_sdk/a/b.Thing is reachable at the deep namespace", () => {
    expect(Thing).toBeDefined();
  });
});
