/** Shared callback re-entry contracts, independent of a language's call mode. */
import "./baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import * as baml from "./baml_sdk/host_callable_tests/index.js";

describe("callable_reentry", () => {
  it("callback_reenters_baml", async () => {
    const calls: Array<[string, number]> = [];
    const leaf = async (value: number) => {
      calls.push(["leaf", value]);
      await Promise.resolve();
      return value + 1;
    };
    const callback = async (value: number) => {
      calls.push(["outer", value]);
      return await baml.call_int_callback_async(
        leaf as unknown as (value: number) => number,
        value,
      );
    };
    expect(
      await baml.call_int_callback_async(
        callback as unknown as (value: number) => number,
        6,
      ),
    ).toBe(7);
    expect(calls).toEqual([
      ["outer", 6],
      ["leaf", 6],
    ]);
  });

  it("same_callback_recurses_through_baml", async () => {
    const calls: number[] = [];
    const callback = async (value: number): Promise<number> => {
      calls.push(value);
      await Promise.resolve();
      return value === 0
        ? 7
        : await baml.call_int_callback_async(
            callback as unknown as (value: number) => number,
            value - 1,
          );
    };
    expect(
      await baml.call_int_callback_async(
        callback as unknown as (value: number) => number,
        3,
      ),
    ).toBe(7);
    expect(calls).toEqual([3, 2, 1, 0]);
  });
});
