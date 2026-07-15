import "./baml_sdk/index.js";
import {
  BamlCallContext,
  BamlCancelledError,
} from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import { SleepMs, SleepMs_async } from "./baml_sdk/throws_test/index.js";

const MAX_CANCELLATION_MS = 500;
const nodeNA = it;

function expectAbortError(error: unknown): void {
  expect(error).toBeInstanceOf(Error);
  expect((error as Error).name).toBe("AbortError");
}

function expectBamlCancelledReason(error: unknown): void {
  expectAbortError(error);
  expect((error as { reason?: unknown }).reason).toBeInstanceOf(BamlCancelledError);
}

function expectFastCancellation(start: number): void {
  expect(performance.now() - start).toBeLessThan(MAX_CANCELLATION_MS);
}

describe("function_calls — cancellation parity", () => {
  it("test_sync_call_returns_none", () => {
    expect(SleepMs(1)).toBeNull();
  });

  it("test_async_call_returns_none", async () => {
    await expect(SleepMs_async(1)).resolves.toBeNull();
  });

  it("test_sync_cancel_via_call_context", () => {
    const start = performance.now();
    // Node's synchronous bridge blocks the event loop, so a timer cannot abort
    // an in-flight sync call. A pre-aborted generated call is the executable
    // host analogue and proves the `$ctx` path reaches BAML cancellation.
    const ctx = new BamlCallContext();
    ctx.abort();

    try {
      SleepMs(2_000, { $ctx: ctx });
      throw new Error("expected generated sync call to throw");
    } catch (error) {
      expectBamlCancelledReason(error);
    }
    expectFastCancellation(start);
  });

  it("test_async_cancel_via_call_context", async () => {
    const start = performance.now();
    const ctx = new BamlCallContext();
    const pending = SleepMs_async(2_000, { $ctx: ctx });

    await new Promise((resolve) => setTimeout(resolve, 50));
    ctx.abort();
    try {
      await pending;
      throw new Error("expected generated async call to reject");
    } catch (error) {
      expectBamlCancelledReason(error);
    }
    expectFastCancellation(start);
  });

  nodeNA("test_async_cancel_via_task_cancel", async () => {
    const ctx = new BamlCallContext();
    const pending = SleepMs_async(2_000, { $ctx: ctx });

    // Unlike asyncio.Task, a Promise has no cancellation operation. This is a
    // host-language N/A; AbortSignal is the Node analogue. Clean up through the
    // supported call-context path so the test never leaks a two-second call.
    expect((pending as Promise<null> & { cancel?: unknown }).cancel).toBeUndefined();
    ctx.abort();
    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
  });

  it("test_async_cancel_via_task_group_sibling", async () => {
    const start = performance.now();
    const ctx = new BamlCallContext();
    const pending = SleepMs_async(2_000, { $ctx: ctx });
    const failSoon = new Promise<never>((_resolve, reject) => {
      setTimeout(() => {
        ctx.abort();
        reject(new Error("cancel siblings"));
      }, 50);
    });

    await expect(Promise.all([pending, failSoon])).rejects.toThrow("cancel siblings");
    expectFastCancellation(start);
  });

  it("test_async_cancel_via_asyncio_timeout", async () => {
    const start = performance.now();
    const ctx = new BamlCallContext();
    const pending = SleepMs_async(2_000, { $ctx: ctx });
    const timer = setTimeout(() => ctx.abort(), 50);

    try {
      await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    } finally {
      clearTimeout(timer);
    }
    expectFastCancellation(start);
  });
});
