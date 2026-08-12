// Roundtrip coverage for baml_sdk/unions — ported from test_unions.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  T,
  UnionContainer,
  round_trip_null_to_end,
  round_trip_dedup,
  round_trip_singleton_unwrap,
  round_trip_optional_plus_null,
  round_trip_t,
  round_trip_union_container,
} from "./baml_sdk/unions/index.js";

describe("roundtrip unions", () => {
  it("unions_round_trip_null_to_end", () => {
    expect(round_trip_null_to_end(1)).toBe(1);
    expect(round_trip_null_to_end("s")).toBe("s");
    expect(round_trip_null_to_end(null)).toBeNull();
  });
  it("unions_round_trip_dedup", () => {
    expect(round_trip_dedup(2)).toBe(2);
    expect(round_trip_dedup("x")).toBe("x");
  });
  it("unions_round_trip_singleton_unwrap", () =>
    expect(round_trip_singleton_unwrap(7)).toBe(7));
  it("unions_round_trip_optional_plus_null", () => {
    const t = new T({ v: 1 });
    expect(round_trip_optional_plus_null(t)).toEqual(t);
    expect(round_trip_optional_plus_null("s")).toBe("s");
    expect(round_trip_optional_plus_null(null)).toBeNull();
  });
  it("unions_round_trip_t", () => {
    const t = new T({ v: 4 });
    expect(round_trip_t(t)).toEqual(t);
  });
  it("unions_round_trip_union_container", () => {
    const c = new UnionContainer({
      null_to_end: null,
      dedup: "d",
      singleton_unwrap: 5,
      optional_plus_null: new T({ v: 2 }),
    });
    expect(round_trip_union_container(c)).toEqual(c);
  });
});
