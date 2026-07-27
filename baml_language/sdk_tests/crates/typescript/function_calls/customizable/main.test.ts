// Smoke tests for plain (non-LLM) expression functions.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import {
  hello_world,
  hello_world_async,
  round_trip_bool_async,
  round_trip_float_async,
  round_trip_int_async,
  round_trip_string_async,
  single_required_arg,
} from "./baml_sdk/index.js";

describe("function_calls — hello_world", () => {
  it("main_returns_the_literal_sync", () => {
    expect(hello_world()).toBe("hello world");
  });

  it("main_returns_the_literal_async", async () => {
    expect(await hello_world_async()).toBe("hello world");
  });
});

describe("function_calls — single_required_arg", () => {
  it("main_round_trips_a_single_positional_argument", () => {
    // The next step up from the nullary case: one required positional arg.
    expect(single_required_arg("hi")).toBe("hi");
  });
});

describe("function_calls — primitive arguments across callFunction", () => {
  it("main_round_trips_ints_bools_strings_and_floats", async () => {
    expect(await round_trip_int_async(42)).toBe(42);
    expect(await round_trip_bool_async(false)).toBe(false);
    expect(await round_trip_string_async("hello from the browser")).toBe(
      "hello from the browser",
    );
    expect(await round_trip_float_async(3.25)).toBeCloseTo(3.25);
  });
});
