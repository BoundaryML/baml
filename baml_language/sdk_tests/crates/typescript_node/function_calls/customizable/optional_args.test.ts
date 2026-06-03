// TypeScript correspondent to test_optional_args.py: host calls against
// BAML functions with optional (default-valued) parameters.
//
//   function scale(base: int, factor: int = 2) -> int { base * factor }
//   function classify(value: int? = 7) -> int { ... }
//
// NOTE on the generated surface type: `sdkgen_typescript_node` does not yet
// thread argument defaults into the `as (...) => ...` cast, so every
// parameter is rendered *required* — `scale` types as `(base, factor)` and
// `classify` as `(value)`. The explicit-argument paths below are therefore
// fully typed. Omitting a defaulted argument so the engine fills the
// BAML-side default is a runtime capability of the binding
// (`defineFunction` only encodes the args actually supplied) that the
// current type can't express, so those cases go through a deliberate cast
// that drops the optional parameter — exactly the shape a future
// optional-aware surface type will allow. The Python suite exercises
// omission directly.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { add_five, classify, scale, tag } from "./baml_sdk/index.js";

describe("function_calls — optional args, explicit values", () => {
  it("passes a defaulted parameter explicitly", () => {
    expect(scale(5, 2)).toBe(10);
    expect(scale(5, 3)).toBe(15);
    expect(add_five(10, 5)).toBe(15);
    expect(add_five(10, 3)).toBe(13);
  });

  it("distinguishes an explicit null from a supplied value", () => {
    // classify's default `7` is never reached here — both args are explicit.
    expect(classify(null)).toBe(-1);
    expect(classify(5)).toBe(5);
    // tag's null default is likewise bypassed when prefix is explicit.
    expect(tag("widget", null)).toBe("widget");
    expect(tag("widget", "ui")).toBe("ui:widget");
  });
});

describe("function_calls — optional args, engine-filled defaults", () => {
  // The cast drops the optional parameter from the call signature; at
  // runtime the binding encodes only `base`/no args, and the engine fills
  // the omitted default. This is the TS analog of `scale(5)` / `classify()`
  // in Python.
  it("omits a defaulted parameter and lets the engine fill it", () => {
    const scaleBaseOnly = scale as unknown as (base: number) => number;
    expect(scaleBaseOnly(5)).toBe(10); // factor defaults to 2

    const classifyOmitted = classify as unknown as () => number;
    expect(classifyOmitted()).toBe(7); // value defaults to 7, not null

    const addFiveBaseOnly = add_five as unknown as (base: number) => number;
    expect(addFiveBaseOnly(10)).toBe(15); // addend defaults to 5

    const tagNameOnly = tag as unknown as (name: string) => string;
    expect(tagNameOnly("widget")).toBe("widget"); // prefix defaults to null
  });
});
