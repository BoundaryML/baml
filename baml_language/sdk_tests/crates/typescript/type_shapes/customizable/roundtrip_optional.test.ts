// Roundtrip coverage for baml_sdk/optional — ported from test_optional.py.
import "./baml_sdk/index.js";
import { BamlError } from "@boundaryml/baml-bridge";
import { describe, it, expect } from "vitest";
import {
  Resume,
  OptionalContainer,
  round_trip_optional_int,
  round_trip_optional_resume,
  round_trip_optional_union,
  round_trip_resume,
  round_trip_optional_container,
} from "./baml_sdk/optional/index.js";

describe("roundtrip optional", () => {
  it("optional_round_trip_optional_int", () => {
    expect(round_trip_optional_int(5)).toBe(5);
    expect(round_trip_optional_int(null)).toBeNull();
  });
  it("optional_round_trip_optional_resume", () => {
    const r = new Resume({ name: "ada" });
    expect(round_trip_optional_resume(r)).toEqual(r);
    expect(round_trip_optional_resume(null)).toBeNull();
  });
  it("optional_round_trip_optional_union", () => {
    expect(round_trip_optional_union(3)).toBe(3);
    expect(round_trip_optional_union("s")).toBe("s");
    expect(round_trip_optional_union(null)).toBeNull();
  });
  it("optional_round_trip_resume", () => {
    const r = new Resume({ name: "grace" });
    expect(round_trip_resume(r)).toEqual(r);
  });
  it("optional_round_trip_optional_container", () => {
    const c = new OptionalContainer({
      optional_int: null,
      optional_class: new Resume({ name: "x" }),
      optional_union: "y",
    });
    expect(round_trip_optional_container(c)).toEqual(c);
  });
  // SDK_PARITY_LINT(skip): TypeScript object literals and constructor inits may leave nullable keys out
  it("optional_omitted_nullable_keys_read_as_null", () => {
    const allNull = new OptionalContainer({
      optional_int: null,
      optional_class: null,
      optional_union: null,
    });
    // A plain object and a constructor init both type-check without the
    // nullable keys, and the runtime reads each omitted key as `null`.
    expect(round_trip_optional_container({})).toEqual(allNull);
    expect(round_trip_optional_container(new OptionalContainer({}))).toEqual(allNull);
    expect(round_trip_optional_container({ optional_class: { name: "x" } })).toEqual(
      new OptionalContainer({
        optional_int: null,
        optional_class: new Resume({ name: "x" }),
        optional_union: null,
      }),
    );
  });
  // SDK_PARITY_LINT(skip): only an untyped TypeScript object can leave a required key out
  it("optional_omitted_required_key_is_rejected", () => {
    let thrown: unknown;
    try {
      round_trip_resume({} as unknown as Resume);
    } catch (error) {
      thrown = error;
    }
    expect(thrown).toBeInstanceOf(BamlError);
    expect((thrown as BamlError).className).toBe("baml.errors.TypeMismatch");
    expect((thrown as BamlError).value).toMatchObject({
      message: expect.stringContaining("Missing field `name`"),
    });
  });
});
