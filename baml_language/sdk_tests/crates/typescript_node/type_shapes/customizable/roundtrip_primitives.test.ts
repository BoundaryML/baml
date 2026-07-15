// Roundtrip coverage for baml_sdk/primitives — ported from
// python_pydantic2/.../roundtrip_tests/test_primitives.py.
import "./baml_sdk/index.js"; // initializes the BAML runtime
import { describe, it, expect } from "vitest";
import {
  Primitives,
  return_int,
  return_float,
  return_string,
  return_bool,
  return_bigint,
  return_null,
  round_trip_uint8_array,
  round_trip_int,
  round_trip_bigint,
  round_trip_float,
  round_trip_string,
  round_trip_bool,
  round_trip_null,
  round_trip_primitives,
} from "./baml_sdk/primitives/index.js";

describe("roundtrip primitives", () => {
  it("test_return_int", () => expect(return_int()).toBeCloseTo(42));
  it("test_return_float", () => expect(return_float()).toBeCloseTo(3.14));
  it("test_return_string", () => expect(return_string()).toBe("hello"));
  it("test_return_bool", () => expect(return_bool()).toBe(true));
  it("test_return_null", () => expect(return_null()).toBeNull());
  it("test_return_bigint", () =>
    expect(return_bigint()).toBe(9223372036854775808n));

  it("test_round_trip_int", () => expect(round_trip_int(7)).toBeCloseTo(7));
  it("test_round_trip_bigint_zero", () => expect(round_trip_bigint(0n)).toBe(0n));
  it("test_round_trip_bigint_negative", () =>
    expect(round_trip_bigint(-9223372036854775809n)).toBe(-9223372036854775809n));
  it("test_round_trip_bigint_larger_than_i64", () =>
    expect(round_trip_bigint(2n ** 100n)).toBe(2n ** 100n));
  it("test_round_trip_float", () =>
    expect(round_trip_float(2.5)).toBeCloseTo(2.5));
  it("test_round_trip_float_accepts_int", () => {
    const result = round_trip_float(7);
    expect(typeof result).toBe("number");
    expect(result).toBe(7);
  });
  it("test_round_trip_float_accepts_large_integral_value", () => {
    const value = 2 ** 54;
    const result = round_trip_float(value);
    expect(typeof result).toBe("number");
    expect(result).toBe(value);
  });
  it("test_round_trip_string", () =>
    expect(round_trip_string("hi")).toBe("hi"));
  it("test_round_trip_bool", () => expect(round_trip_bool(false)).toBe(false));
  it("test_round_trip_null", () => expect(round_trip_null(null)).toBeNull());

  it("test_round_trip_uint8_array", () => {
    const r = round_trip_uint8_array(new Uint8Array([0, 1, 2]));
    expect(Array.from(r as Uint8Array)).toEqual([0, 1, 2]);
  });

  it("test_round_trip_primitives", () => {
    const p = new Primitives({
      int_field: 1,
      float_field: 1.5,
      string_field: "s",
      bool_field: true,
      null_field: null,
      uint8array_field: new Uint8Array([97, 98]),
      bigint_field: 2n ** 100n,
    });
    const r = round_trip_primitives(p);
    expect(r).toBeInstanceOf(Primitives);
    expect(r.int_field).toBeCloseTo(1);
    expect(r.float_field).toBeCloseTo(1.5);
    expect(r.string_field).toBe("s");
    expect(r.bool_field).toBe(true);
    expect(r.null_field).toBeNull();
    expect(Array.from(r.uint8array_field as Uint8Array)).toEqual([97, 98]);
    expect(r.bigint_field).toBe(2n ** 100n);
  });

  it("test_round_trip_primitives_float_field_accepts_int", () => {
    const p = new Primitives({
      int_field: 1,
      float_field: 2,
      string_field: "s",
      bool_field: true,
      null_field: null,
      uint8array_field: new Uint8Array([97, 98]),
      bigint_field: 2n ** 100n,
    });
    expect(typeof p.float_field).toBe("number");
    const result = round_trip_primitives(p);
    expect(result.float_field).toBe(2);
  });
});
