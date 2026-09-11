// A BAML-thrown value is decoded through the generated class map before it
// is surfaced as a `BamlError`. When that decode fails — here, the mapped
// class's constructor throws — the failure must reach the caller: the bridge
// used to swallow it and surface a `BamlError` whose `.value` was
// `undefined`, hiding the only diagnostic.
import "./baml_sdk/index.js";
import { BamlTypeMap, getTypeMap, setTypeMap } from "@boundaryml/baml-bridge";
import { afterEach, describe, expect, it } from "vitest";
import { ThrowMyError_async } from "./baml_sdk/throws_test/index.js";

describe("function_calls — thrown value decoding", () => {
  const generated = getTypeMap();
  afterEach(() => setTypeMap(generated));

  it("thrown_value_decode_failure_propagates_instead_of_surfacing_an_empty_error", async () => {
    class BrokenMyError {
      constructor() {
        throw new Error("MyError constructor failed");
      }
    }
    setTypeMap(
      BamlTypeMap.fromLazyEntries({
        classes: { "user.throws_test.MyError": () => BrokenMyError },
        enums: {},
        typeAliases: {},
      }),
    );

    await expect(ThrowMyError_async()).rejects.toThrow("MyError constructor failed");
  });
});
