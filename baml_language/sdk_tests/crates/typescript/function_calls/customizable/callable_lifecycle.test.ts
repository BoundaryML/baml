import { CancelToken } from "./baml_sdk/baml/spawn/index.js";
/** Shared retained-callable and cancelled-waiter contracts.
 * No particular async closure API or callback cancellation mechanism is
 * required. Native-blocking regressions need an external process deadline.
 */
import "./baml_sdk/index.js";
import { BamlAbortError } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import * as baml from "./baml_sdk/host_callable_tests/index.js";

describe("callable_lifecycle", () => {
  it("returned_closure_retains_host_callback", async () => {
    const calls: number[] = [];
    const callback = (value: number) => {
      calls.push(value);
      return value + 1;
    };
    const forward = await baml.make_callback_forwarder_async(callback);
    expect(calls).toEqual([]);
    expect(await baml.call_int_callback_async(forward, 6)).toBe(7);
    expect(await baml.call_int_callback_async(forward, 7)).toBe(8);
    expect(calls).toEqual([6, 7]);
  });

  it("returned_closure_preserves_callback_error_and_remains_reusable", async () => {
    const failure = new Error("retained failure");
    const callback = (value: number) => {
      if (value === 1) throw failure;
      return value;
    };
    const forward = await baml.make_callback_forwarder_async(callback);
    await expect(baml.call_int_callback_async(forward, 1)).rejects.toBe(
      failure,
    );
    expect(await baml.call_int_callback_async(forward, 2)).toBe(2);
  });

  it("cancelled_waiter_stays_cancelled_when_callback_returns_late", async () => {
    const ctx = CancelToken.new();
    let enter!: () => void;
    const entered = new Promise<void>((resolve) => {
      enter = resolve;
    });
    let release!: () => void;
    const callbackWait = new Promise<void>((resolve) => {
      release = resolve;
    });
    let exit!: () => void;
    const exited = new Promise<void>((resolve) => {
      exit = resolve;
    });
    const callback = async (value: number) => {
      enter();
      await callbackWait;
      exit();
      return value;
    };
    const call = baml.call_int_callback_async(
      callback as unknown as (value: number) => number,
      1,
      { $baml: { cancel: ctx } },
    );
    const outcome = call.catch((error: unknown) => error);
    try {
      await entered;
      ctx.cancel();
      const cancellation = await outcome;
      expect(cancellation).toBeInstanceOf(BamlAbortError);
      release();
      await exited;
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      expect(await outcome).toBe(cancellation);
      expect(await baml.call_int_callback_async((value) => value, 2)).toBe(2);
    } finally {
      release();
      ctx.cancel();
      await outcome;
    }
  });
});
