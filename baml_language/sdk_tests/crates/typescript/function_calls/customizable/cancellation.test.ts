import { CancelToken } from "./baml_sdk/baml/spawn/index.js";
import "./baml_sdk/index.js";
import {
  BamlAbortError,
  BamlCancelledError,
  callFunction,
  callFunctionSync,
  getOrInitRuntime,
} from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import { Greeter } from "./baml_sdk/methods_on_classes/index.js";
import { SleepMs } from "./baml_sdk/throws_test/index.js";
import { call_with_callback_async } from "./baml_sdk/host_callable_tests/index.js";

const SLEEP_FQN = "user.throws_test.SleepMs";
const HOST_CALLBACK_FQN = "user.host_callable_tests.call_with_callback";
// The sleeping cancelled calls below (or the hang on a pending host
// callback): the operation must dwarf this bound, or a regression that
// ignored cancellation would still finish inside it and pass.
const MAX_CANCELLATION_MS = 5000;

function expectAbortError(error: unknown): void {
  expect(error).toBeInstanceOf(Error);
  expect((error as Error).name).toBe("AbortError");
}

function expectBamlCancelledReason(error: unknown): void {
  expectAbortError(error);
  expect(error).toBeInstanceOf(BamlAbortError);
  expect((error as { reason?: unknown }).reason).toBeInstanceOf(
    BamlCancelledError,
  );
  const reason = (error as BamlAbortError).reason as BamlCancelledError;
  expect(reason.name).toBe("BamlCancelledError");
  expect(reason.bamlTrace).toEqual([]);
  expect(reason.className).toBeUndefined();
  expect(reason.value).toBeUndefined();
}

function expectFastCancellation(start: number): void {
  expect(performance.now() - start).toBeLessThan(MAX_CANCELLATION_MS);
}

function pendingIntCallback(): {
  callback: (value: number) => string;
  dispatched: Promise<void>;
} {
  let markDispatched!: () => void;
  const dispatched = new Promise<void>((resolve) => { markDispatched = resolve; });
  const callback = ((_value: number) => new Promise<string>(() => {
    markDispatched();
  })) as unknown as (value: number) => string;
  return { callback, dispatched };
}

function pendingNoArgCallback(): {
  callback: () => string;
  dispatched: Promise<void>;
} {
  let markDispatched!: () => void;
  const dispatched = new Promise<void>((resolve) => { markDispatched = resolve; });
  const callback = (() => new Promise<string>(() => {
    markDispatched();
  })) as unknown as () => string;
  return { callback, dispatched };
}

describe(
  "function_calls — explicit runtime cancellation",
  () => {
    it("cancellation_surfaces_sync_pre_aborted_cancellation_as_abort_error", () => {
      const start = performance.now();
      // True in-flight sync cancellation cannot be scheduled from JS: the sync
      // bridge blocks the Node main thread, so setTimeout/setImmediate and
      // ThreadsafeFunction deliveries cannot run until callFunctionSync returns.
      // A pre-aborted context still exercises the sync cancellation path.
      const ctx = CancelToken.new();
      ctx.cancel();

      try {
        callFunctionSync(
          getOrInitRuntime(),
          SLEEP_FQN,
          { ms: 60000 },
          { cancel: ctx },
        );
        throw new Error("expected callFunctionSync to throw");
      } catch (error) {
        expectBamlCancelledReason(error);
      }

      expectFastCancellation(start);
    });

    it("cancellation_surfaces_async_cancellation_as_abort_error_with_baml_reason", async () => {
      const start = performance.now();
      const ctx = CancelToken.new();
      const host = pendingIntCallback();
      const pending = callFunction(
        getOrInitRuntime(),
        HOST_CALLBACK_FQN,
        { callback: host.callback, x: 1 },
        { cancel: ctx },
      );

      await host.dispatched;
      ctx.cancel();

      try {
        await pending;
        throw new Error("expected callFunction to reject");
      } catch (error) {
        expectBamlCancelledReason(error);
      }

      expectFastCancellation(start);
    });

    it("cancellation_can_pre_abort_async_baml_cancellation", async () => {
      const start = performance.now();
      const ctx = CancelToken.new();
      ctx.cancel();

      try {
        await callFunction(
          getOrInitRuntime(),
          HOST_CALLBACK_FQN,
          { callback: (value: number) => `${value}`, x: 1 },
          { cancel: ctx },
        );
        throw new Error("expected callFunction to reject");
      } catch (error) {
        expectAbortError(error);
      }

      expectFastCancellation(start);
    });

    it("cancellation_surfaces_cancellation_through_promise_all", async () => {
      const start = performance.now();
      const ctx = CancelToken.new();
      const first = pendingIntCallback();
      const second = pendingIntCallback();
      const pending = Promise.all([
        callFunction(
          getOrInitRuntime(),
          HOST_CALLBACK_FQN,
          { callback: first.callback, x: 1 },
          { cancel: ctx },
        ),
        callFunction(
          getOrInitRuntime(),
          HOST_CALLBACK_FQN,
          { callback: second.callback, x: 2 },
          { cancel: ctx },
        ),
      ]);

      await Promise.all([first.dispatched, second.dispatched]);
      ctx.cancel();

      try {
        await pending;
        throw new Error("expected Promise.all to reject");
      } catch (error) {
        expectBamlCancelledReason(error);
      }

      expectFastCancellation(start);
    });

    it("cancellation_surfaces_a_reused_call_context_as_abort_error_with_baml_reason", async () => {
      const ctx = CancelToken.new();
      const host = pendingIntCallback();
      const pending = callFunction(
        getOrInitRuntime(),
        HOST_CALLBACK_FQN,
        { callback: host.callback, x: 1 },
        { cancel: ctx },
      );

      await host.dispatched;
      ctx.cancel();

      try {
        await pending;
        throw new Error("expected callFunction to reject");
      } catch (error) {
        expectBamlCancelledReason(error);
      }
    });
  },
);

describe("function_calls — generated call cancellation", () => {
  it("cancellation_cancels_an_async_generated_free_function", async () => {
    const ctx = CancelToken.new();
    const host = pendingIntCallback();
    const pending = call_with_callback_async(host.callback, 1, { $baml: { cancel: ctx } });
    await host.dispatched;
    ctx.cancel();
    await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
  });

  it("cancellation_cancels_an_async_generated_instance_method", async () => {
    const ctx = CancelToken.new();
    const greeter = new Greeter({ name: "cancel-me" });
    const host = pendingNoArgCallback();
    const pending = greeter.wait_async(host.callback, { $baml: { cancel: ctx } });
    await host.dispatched;
    ctx.cancel();
    await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
  });

  it("cancellation_pre_aborts_a_generated_synchronous_call", () => {
    const ctx = CancelToken.new();
    ctx.cancel();
    expect(() => SleepMs(2000, { $baml: { cancel: ctx } })).toThrow(BamlAbortError);
  });

  it("cancellation_detaches_a_completed_call_before_aborting_the_remaining_call", async () => {
    const ctx = CancelToken.new();
    await expect(call_with_callback_async((value) => `${value}`, 1, { $baml: { cancel: ctx } })).resolves.toBe("1");
    const host = pendingIntCallback();
    const pending = call_with_callback_async(host.callback, 2, { $baml: { cancel: ctx } });
    await host.dispatched;
    ctx.cancel();
    await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
  });

  it("cancellation_immediately_cancels_every_call_attached_after_abort", async () => {
    const ctx = CancelToken.new();
    ctx.cancel();
    await expect(Promise.all([
      call_with_callback_async((value) => `${value}`, 1, { $baml: { cancel: ctx } }),
      call_with_callback_async((value) => `${value}`, 2, { $baml: { cancel: ctx } }),
    ])).rejects.toBeInstanceOf(BamlAbortError);
    ctx.cancel();
  });
});
