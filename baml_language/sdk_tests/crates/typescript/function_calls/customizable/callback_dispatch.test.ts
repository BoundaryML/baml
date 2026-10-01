/** Shared callable dispatch contracts. Names/assertions match every SDK port.
 * Use the language's supported entry/callback forms; do not require a
 * particular thread, event loop, or context carrier in these shared cases.
 * Casts only admit Promise results omitted by current generated callback types.
 */
import "./baml_sdk/index.js";
import { BamlCallContext, BamlAbortError } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import * as baml from "./baml_sdk/host_callable_tests/index.js";

describe("callback_dispatch", () => {
  it("callable_entry_invokes_callback", async () => {
    const calls: number[] = [];
    const callback = (value: number) => {
      calls.push(value);
      return value + 1;
    };
    expect(await baml.call_int_callback_async(callback, 6)).toBe(7);
    expect(calls).toEqual([6]);
  });

  it("callable_entry_waits_for_callback_completion", async () => {
    const calls: number[] = [];
    const callback = async (value: number) => {
      await Promise.resolve();
      calls.push(value);
      return value + 1;
    };
    expect(
      await baml.call_int_callback_async(
        callback as unknown as (value: number) => number,
        6,
      ),
    ).toBe(7);
    expect(calls).toEqual([6]);
  });

  it("concurrent_calls_keep_callback_results_independent", async () => {
    const arrivals: number[] = [];
    let release!: () => void;
    const allEntered = new Promise<void>((resolve) => {
      release = resolve;
    });
    const callback = async (value: number) => {
      arrivals.push(value);
      if (arrivals.length === 4) release();
      await allEntered;
      return value + 1;
    };
    try {
      const calls = [0, 1, 2, 3].map((value) =>
        baml.call_int_callback_async(
          callback as unknown as (value: number) => number,
          value,
        ),
      );
      expect(await Promise.all(calls)).toEqual([1, 2, 3, 4]);
      expect(arrivals.sort()).toEqual([0, 1, 2, 3]);
    } finally {
      release();
    }
  });

  it("completed_call_can_reuse_callback", async () => {
    const calls: number[] = [];
    const callback = (value: number) => {
      calls.push(value);
      return value + 1;
    };
    expect(await baml.call_int_callback_async(callback, 1)).toBe(2);
    expect(await baml.call_int_callback_async(callback, 2)).toBe(3);
    expect(calls).toEqual([1, 2]);
  });

  it("repeated_dispatches_invoke_callback_in_order", async () => {
    const calls: number[] = [];
    const callback = (value: number) => {
      calls.push(value);
      return String(value);
    };
    expect(await baml.call_repeatedly_async(callback, 3)).toEqual([
      "0",
      "1",
      "2",
    ]);
    expect(calls).toEqual([0, 1, 2]);
  });
  it("cancelled_waiter_does_not_end_host_execution", async () => {
    for (const outcome of ["return", "throw"]) {
      const ctx = new BamlCallContext();
      let markEntered!: () => void;
      const entered = new Promise<void>((resolve) => { markEntered = resolve; });
      let release!: () => void;
      const released = new Promise<void>((resolve) => { release = resolve; });
      let markExited!: () => void;
      const exited = new Promise<void>((resolve) => { markExited = resolve; });
      const exits: number[] = [];
      const callback = async (value: number) => {
        markEntered();
        try {
          await released;
        } finally {
          exits.push(value);
          markExited();
        }
        if (outcome === "throw") throw new Error("late host failure");
        return value + 1;
      };
      const pending = baml.call_int_callback_async(
        callback as unknown as (value: number) => number, 6, { $ctx: ctx },
      );
      try {
        await entered;
        ctx.abort();
        await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
        expect(exits).toEqual([]);
        release();
        await exited;
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
        expect(exits).toEqual([6]);
        await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
        expect(await baml.call_int_callback_async((value) => value + 1, 7)).toBe(8);
      } finally {
        release();
        await pending.catch(() => {});
      }
    }
  });
});
