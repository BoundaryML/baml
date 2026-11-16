// Roundtrip coverage for baml_sdk/primitives — ported from
// python_pydantic2/.../roundtrip_tests/test_primitives.py.
import "./baml_sdk/index.js"; // initializes the BAML runtime
import { describe, it, expect } from "vitest";
import {
  Primitives,
  return_int_sync,
  return_float_sync,
  return_string_sync,
  return_bool_sync,
  return_null_sync,
  round_trip_uint8_array_sync,
  round_trip_int_sync,
  round_trip_float_sync,
  round_trip_string_sync,
  round_trip_bool_sync,
  round_trip_null_sync,
  round_trip_primitives_sync,
} from "./baml_sdk/primitives/index.js";

describe("roundtrip primitives", () => {
  it("primitives_return_int", () => expect(return_int_sync()).toBeCloseTo(42));
  it("primitives_return_float", () => expect(return_float_sync()).toBeCloseTo(3.14));
  it("primitives_return_string", () => expect(return_string_sync()).toBe("hello"));
  it("primitives_return_bool", () => expect(return_bool_sync()).toBe(true));
  it("primitives_return_null", () => expect(return_null_sync()).toBeNull());

  it("primitives_round_trip_int", () => expect(round_trip_int_sync(7)).toBeCloseTo(7));
  it("primitives_round_trip_float", () =>
    expect(round_trip_float_sync(2.5)).toBeCloseTo(2.5));
  it("primitives_round_trip_string", () => expect(round_trip_string_sync("hi")).toBe("hi"));
  it("primitives_round_trip_bool", () => expect(round_trip_bool_sync(false)).toBe(false));
  it("primitives_round_trip_null", () => expect(round_trip_null_sync(null)).toBeNull());

  it("primitives_round_trip_uint8_array", () => {
    const r = round_trip_uint8_array_sync(new Uint8Array([0, 1, 2]));
    expect(Array.from(r as Uint8Array)).toEqual([0, 1, 2]);
  });

  it("primitives_round_trip_primitives", () => {
    const p = new Primitives({
      int_field: 1,
      float_field: 1.5,
      string_field: "s",
      bool_field: true,
      null_field: null,
      uint8array_field: new Uint8Array([97, 98]),
    });
    const r = round_trip_primitives_sync(p);
    expect(r).toBeInstanceOf(Primitives);
    expect(r.int_field).toBeCloseTo(1);
    expect(r.float_field).toBeCloseTo(1.5);
    expect(r.string_field).toBe("s");
    expect(r.bool_field).toBe(true);
    expect(r.null_field).toBeNull();
    expect(Array.from(r.uint8array_field as Uint8Array)).toEqual([97, 98]);
  });
});
