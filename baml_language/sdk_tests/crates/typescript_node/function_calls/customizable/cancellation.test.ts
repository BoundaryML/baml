import "./baml_sdk/index.js";
import {
  BamlCallContext,
  BamlCancelledError,
  callFunction,
  callFunctionSync,
  getRuntime,
} from "@boundaryml/baml-core-node";
import { describe, expect, it } from "vitest";

const SLEEP_FQN = "user.throws_test.SleepMs";
const MAX_CANCELLATION_MS = 500;

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

describe("function_calls — explicit runtime cancellation", () => {
  it("surfaces sync pre-aborted cancellation as AbortError", () => {
    const start = performance.now();
    // True in-flight sync cancellation cannot be scheduled from JS: the sync
    // bridge blocks the Node main thread, so setTimeout/setImmediate and
    // ThreadsafeFunction deliveries cannot run until callFunctionSync returns.
    // A pre-aborted context still exercises the sync cancellation path.
    const ctx = new BamlCallContext();
    ctx.abort();

    try {
      callFunctionSync(
        getRuntime(),
        SLEEP_FQN,
        { ms: 2000 },
        undefined,
        undefined,
        ctx,
      );
      throw new Error("expected callFunctionSync to throw");
    } catch (error) {
      expectBamlCancelledReason(error);
    }

    expectFastCancellation(start);
  });

  it("surfaces async cancellation as AbortError with BAML reason", async () => {
    const start = performance.now();
    const ctx = new BamlCallContext();
    const pending = callFunction(
      getRuntime(),
      SLEEP_FQN,
      { ms: 2000 },
      undefined,
      undefined,
      ctx,
    );

    await new Promise((resolve) => setTimeout(resolve, 50));
    ctx.abort();

    try {
      await pending;
      throw new Error("expected callFunction to reject");
    } catch (error) {
      expectBamlCancelledReason(error);
    }

    expectFastCancellation(start);
  });

  it("can pre-abort async BAML cancellation", async () => {
    const start = performance.now();
    const ctx = new BamlCallContext();
    ctx.abort();

    try {
      await callFunction(
        getRuntime(),
        SLEEP_FQN,
        { ms: 2000 },
        undefined,
        undefined,
        ctx,
      );
      throw new Error("expected callFunction to reject");
    } catch (error) {
      expectAbortError(error);
    }

    expectFastCancellation(start);
  });

  it("surfaces cancellation through Promise.all", async () => {
    const start = performance.now();
    const ctx = new BamlCallContext();
    const pending = Promise.all([
      callFunction(
        getRuntime(),
        SLEEP_FQN,
        { ms: 2000 },
        undefined,
        undefined,
        ctx,
      ),
      callFunction(
        getRuntime(),
        SLEEP_FQN,
        { ms: 2000 },
        undefined,
        undefined,
        ctx,
      ),
    ]);

    await new Promise((resolve) => setTimeout(resolve, 50));
    ctx.abort();

    try {
      await pending;
      throw new Error("expected Promise.all to reject");
    } catch (error) {
      expectBamlCancelledReason(error);
    }

    expectFastCancellation(start);
  });

  it("surfaces a reused call context as AbortError with BAML reason", async () => {
    const ctx = new BamlCallContext();
    const pending = callFunction(
      getRuntime(),
      SLEEP_FQN,
      { ms: 2000 },
      undefined,
      undefined,
      ctx,
    );

    await new Promise((resolve) => setTimeout(resolve, 50));
    ctx.abort();

    try {
      await pending;
      throw new Error("expected callFunction to reject");
    } catch (error) {
      expectBamlCancelledReason(error);
    }
  });
});
