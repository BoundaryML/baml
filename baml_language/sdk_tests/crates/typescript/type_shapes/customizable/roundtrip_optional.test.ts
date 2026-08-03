// Roundtrip coverage for baml_sdk/optional — ported from test_optional.py.
import "./baml_sdk/index.js";
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
});
