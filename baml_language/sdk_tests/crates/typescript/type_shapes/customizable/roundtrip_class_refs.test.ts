// Roundtrip coverage for baml_sdk/class_refs — ported from test_class_refs.py.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { Inner, Outer, make_outer_sync, round_trip_inner_sync, round_trip_outer_sync } from "./baml_sdk/class_refs/index.js";

describe("roundtrip class_refs", () => {
  it("class_refs_make_outer", () => {
    const o = make_outer_sync(5);
    expect(o.inner.value).toBe(5);
  });
  it("class_refs_round_trip_inner", () => {
    const i = new Inner({ value: 3 });
    expect(round_trip_inner_sync(i)).toEqual(i);
  });
  it("class_refs_round_trip_outer", () => {
    const o = new Outer({ inner: new Inner({ value: 9 }) });
    expect(round_trip_outer_sync(o)).toEqual(o);
  });
});
