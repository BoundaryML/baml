// TypeScript/Node-only primitive coverage. Buffer is a Node subclass of
// Uint8Array, so the portable SDK surface stays Uint8Array while accepting
// Buffer inputs without copying the wrong portion of a sliced backing store.
import "../baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import {
  return_unsafe_int,
  round_trip_int,
  round_trip_uint8_array,
} from "../baml_sdk/primitives/index.js";

describe("language-specific roundtrip primitives", () => {
  it("accepts a sliced Node Buffer as Uint8Array", () => {
    const backing = Buffer.from([99, 0, 1, 2, 100]);
    const slice = backing.subarray(1, 4);

    const result = round_trip_uint8_array(slice);

    expect(Array.from(result)).toEqual([0, 1, 2]);
  });

  it("rejects unsafe integer number input before precision is lost", () => {
    expect(() => round_trip_int(Number.MAX_SAFE_INTEGER + 1)).toThrow(
      /float value .*BAML int/i,
    );
  });

  it("rejects an outbound int outside the JavaScript safe range", () => {
    expect(() => return_unsafe_int()).toThrow(/outside JavaScript's safe-integer range/i);
  });
});
