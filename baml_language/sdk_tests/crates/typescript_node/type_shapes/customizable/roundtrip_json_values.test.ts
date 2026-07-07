// Roundtrip coverage for baml_sdk/json_values — the stdlib `json` type.
// Ported from python_pydantic2/.../roundtrip_tests/test_json_values.py.
//
// Every BAML function here is declared to return `json` (`baml.json.json`),
// but each returns a differently-shaped value.
import "./baml_sdk/index.js"; // initializes the BAML runtime
import { describe, it, expect } from "vitest";
import {
  JsonContainer,
  return_json_null,
  return_json_bool,
  return_json_int,
  return_json_float,
  return_json_string,
  return_json_array,
  return_json_object,
  return_json_nested,
  round_trip_json,
  round_trip_json_container,
} from "./baml_sdk/json_values/index.js";

describe("roundtrip json_values", () => {
  it("return_json_null", () => expect(return_json_null()).toBeNull());
  it("return_json_bool", () => expect(return_json_bool()).toBe(true));
  it("return_json_int", () => expect(return_json_int()).toBeCloseTo(42));
  it("return_json_float", () => expect(return_json_float()).toBeCloseTo(3.14));
  it("return_json_string", () => expect(return_json_string()).toBe("hello"));
  it("return_json_array", () => expect(return_json_array()).toEqual([1, 2, 3]));
  it("return_json_object", () =>
    expect(return_json_object()).toEqual({ key: "value" }));
  it("return_json_nested", () =>
    expect(return_json_nested()).toEqual({
      a: 1,
      b: [2, 3],
      c: { nested: null },
    }));

  it("round_trip_json null", () => expect(round_trip_json(null)).toBeNull());
  it("round_trip_json bool", () => expect(round_trip_json(false)).toBe(false));
  it("round_trip_json int", () => expect(round_trip_json(7)).toBeCloseTo(7));
  it("round_trip_json float", () =>
    expect(round_trip_json(2.5)).toBeCloseTo(2.5));
  it("round_trip_json string", () => expect(round_trip_json("hi")).toBe("hi"));
  it("round_trip_json array", () =>
    expect(round_trip_json([1, "two", true, null])).toEqual([
      1,
      "two",
      true,
      null,
    ]));
  it("round_trip_json object", () => {
    const nested = { a: 1, b: [2, 3], c: { nested: null } };
    expect(round_trip_json(nested)).toEqual(nested);
  });

  it("round_trip_json_container", () => {
    const c = new JsonContainer({ data: { k: [1, 2, { deep: null }] } });
    const r = round_trip_json_container(c);
    expect(r).toBeInstanceOf(JsonContainer);
    expect(r.data).toEqual({ k: [1, 2, { deep: null }] });
  });
});
