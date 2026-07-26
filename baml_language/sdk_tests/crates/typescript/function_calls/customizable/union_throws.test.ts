// Regression for the bridge-dogfood bug: a throw escaping a *multi-member*
// `throws` union must carry the thrown value's class FQN in `className`,
// exactly like a single-member throws. The engine wraps a thrown value in
// `union_variant_value` for a multi-member `throws`, and `decodeThrown` only
// unwrapped a top-level `class_value` — so `className` came back `undefined`
// for union throws while `.value` still decoded fine.
import "./baml_sdk/index.js";
import { raises_test } from "./baml_sdk/index.js";
import { BamlError } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";

describe("function_calls — union throws className", () => {
  it("errors_union_throws_preserves_class_name", () => {
    // `Reparse` declares `throws ParseError` (single); `LoadDoc` declares
    // `throws ParseError | TimeoutError` (union). Both throw `ParseError`, so
    // the surfaced `className` must agree.
    let single: BamlError | undefined;
    try {
      raises_test.Reparse("x");
    } catch (e) {
      single = e as BamlError;
    }
    let union: BamlError | undefined;
    try {
      raises_test.LoadDoc("x");
    } catch (e) {
      union = e as BamlError;
    }

    expect(single).toBeInstanceOf(BamlError);
    expect(union).toBeInstanceOf(BamlError);
    expect(single!.className).toBe("user.raises_test.ParseError");
    expect(union!.className).toBe(single!.className);
    expect(union!.value).toBeInstanceOf(raises_test.ParseError);
  });
});
