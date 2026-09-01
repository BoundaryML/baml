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
  runtime_identity,
  single_required_arg,
} from "./baml_sdk/index.js";

function decodeRuntimeCallId(value: string): Uint8Array {
  const prefix = "baml_call_1_";
  expect(value.startsWith(prefix)).toBe(true);
  const encoded = value
    .slice(prefix.length)
    .replaceAll("-", "+")
    .replaceAll("_", "/");
  const padded = encoded.padEnd(Math.ceil(encoded.length / 4) * 4, "=");
  return Uint8Array.from(globalThis.atob(padded), (byte) => byte.charCodeAt(0));
}

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

describe("function_calls — runtime identity", () => {
  it("keeps_the_process_and_engine_prefix_stable_across_calls", () => {
    const first = decodeRuntimeCallId(runtime_identity());
    const second = decodeRuntimeCallId(runtime_identity());
    // Wire version + ProcessEuid + EngineId are stable; thread/call IDs vary.
    expect(first.slice(0, 25)).toEqual(second.slice(0, 25));
    expect(first.slice(25)).not.toEqual(second.slice(25));
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
