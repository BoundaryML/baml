import { CancelToken } from "./baml_sdk/baml/spawn/index.js";
/** TypeScript-only callable lifecycle.
 * Node sync entries reject host callbacks. Promises use application-owned
 * AbortSignals; these assertions do not promise bridge-driven cancellation.
 * Native-blocking cases require an external process deadline.
 */
import "./baml_sdk/index.js";
import { BamlAbortError } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import * as baml from "./baml_sdk/host_callable_tests/index.js";
import { isTestRuntime } from "./test_runtime.js";

let AsyncLocalStorage: typeof import("node:async_hooks").AsyncLocalStorage;
let nextTurn: typeof import("node:timers/promises").setImmediate;
if (isTestRuntime("node")) {
  ({ AsyncLocalStorage } = await import(
    /* @vite-ignore */ ["node", "async_hooks"].join(":")
  ));
  ({ setImmediate: nextTurn } = await import(
    /* @vite-ignore */ ["node", "timers/promises"].join(":")
  ));
}

describe.runIf(isTestRuntime("node"))(
  "callable_lifecycle_typescript_only",
  () => {
    // SDK_PARITY_LINT(skip): Node cannot run JS callbacks while a sync native call blocks its event loop
    it("sync_retained_closure_rejects_before_dispatch_typescript_only", async () => {
      const seen: number[] = [];
      const forward = await baml.make_callback_forwarder_async((value) => {
        seen.push(value);
        return value + 1;
      });
      expect(() => forward(1)).toThrow(/host callables.*async/i);
      expect(seen).toEqual([]);
      expect(await baml.call_int_callback_async(forward, 2)).toBe(3);
      expect(seen).toEqual([2]);
    });

    // SDK_PARITY_LINT(skip): Node sync entries must reject callbacks hidden in native closure captures
    it("sync_entry_rejects_retained_callback_and_allows_async_reuse_typescript_only", async () => {
      const seen: number[] = [];
      const forward = await baml.make_callback_forwarder_async((value) => {
        seen.push(value);
        return value + 1;
      });
      expect(() => baml.call_int_callback(forward, 1)).toThrow(
        /host callables.*async/i,
      );
      expect(seen).toEqual([]);
      expect(await baml.call_int_callback_async(forward, 2)).toBe(3);
      expect(seen).toEqual([2]);
    });

    // SDK_PARITY_LINT(skip): Node sync calls support native closures whose captures need no host dispatch
    it("native_closure_can_cross_sync_and_async_entries_typescript_only", async () => {
      const add = baml.make_adder(10);
      expect(baml.call_int_callback(add, 1)).toBe(11);
      expect(await baml.call_int_callback_async(add, 2)).toBe(12);
      expect(add(3)).toBe(13);
      expect(baml.call_int_callback(add, 4)).toBe(14);
    });

    // SDK_PARITY_LINT(skip): Node callback Promises can outlive an aborted SDK waiter
    it("late_callback_keeps_async_local_storage_after_abort_typescript_only", async () => {
      const request = new AsyncLocalStorage<string>();
      const ctx = CancelToken.new();
      const seen: Array<string | undefined> = [];
      let enter!: () => void;
      const entered = new Promise<void>((resolve) => {
        enter = resolve;
      });
      let release!: () => void;
      const wait = new Promise<void>((resolve) => {
        release = resolve;
      });
      let exit!: () => void;
      const exited = new Promise<void>((resolve) => {
        exit = resolve;
      });
      const callback = async (value: number) => {
        seen.push(request.getStore());
        enter();
        await wait;
        seen.push(request.getStore());
        exit();
        return value;
      };
      const call = request.run("entry", () =>
        baml.call_int_callback_async(
          callback as unknown as (value: number) => number,
          1,
          { $baml: { cancel: ctx } },
        ),
      );
      const outcome = call.catch((error: unknown) => error);
      try {
        await entered;
        ctx.cancel();
        expect(await outcome).toBeInstanceOf(BamlAbortError);
        release();
        await exited;
        expect(seen).toEqual(["entry", "entry"]);
        expect(request.getStore()).toBeUndefined();
      } finally {
        release();
        ctx.cancel();
        await outcome;
      }
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("cooperative_abort_cleans_up_retained_callback_typescript_only", async () => {
      // Use the existing async BAML entry with the retained native closure.
      // This must preserve its native handle rather than wrap a sync JS call.
      const ctx = CancelToken.new();
      const application = new AbortController();
      let enter!: () => void;
      const entered = new Promise<void>((resolve) => {
        enter = resolve;
      });
      let exit!: () => void;
      const exited = new Promise<void>((resolve) => {
        exit = resolve;
      });
      const callback = async (value: number) => {
        enter();
        try {
          await new Promise<void>((_resolve, reject) => {
            application.signal.addEventListener(
              "abort",
              () => reject(application.signal.reason),
              { once: true },
            );
          });
          return value;
        } finally {
          exit();
        }
      };
      const forward = await baml.make_callback_forwarder_async(
        callback as unknown as (value: number) => number,
      );
      const call = baml.call_int_callback_async(forward, 1, { $baml: { cancel: ctx } });
      const outcome = call.catch((error: unknown) => error);
      try {
        await entered;
        ctx.cancel();
        application.abort();
        expect(await outcome).toBeInstanceOf(BamlAbortError);
        await exited;
      } finally {
        ctx.cancel();
        application.abort();
        await outcome;
      }
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("repeated_cooperative_abort_does_not_interrupt_callback_cleanup_typescript_only", async () => {
      const ctx = CancelToken.new();
      const application = new AbortController();
      let enter!: () => void;
      const entered = new Promise<void>((resolve) => {
        enter = resolve;
      });
      let clean!: () => void;
      const cleaning = new Promise<void>((resolve) => {
        clean = resolve;
      });
      let release!: () => void;
      const cleanup = new Promise<void>((resolve) => {
        release = resolve;
      });
      let finished = false;
      let exit!: () => void;
      const exited = new Promise<void>((resolve) => {
        exit = resolve;
      });
      const callback = async (value: number) => {
        enter();
        try {
          await new Promise<void>((_resolve, reject) => {
            application.signal.addEventListener(
              "abort",
              () => reject(application.signal.reason),
              { once: true },
            );
          });
          return value;
        } finally {
          clean();
          await cleanup;
          finished = true;
          exit();
        }
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
        application.abort();
        await cleaning;
        ctx.cancel();
        application.abort();
        await nextTurn();
        expect(finished).toBe(false);
        release();
        expect(await outcome).toBeInstanceOf(BamlAbortError);
        await exited;
      } finally {
        release();
        ctx.cancel();
        application.abort();
        await outcome;
      }
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("pre_aborted_call_does_not_dispatch_callback_typescript_only", async () => {
      // Node has no callback Task factory. Its equivalent boundary is a
      // pre-aborted call context: callback must never run, and the next call works.
      const ctx = CancelToken.new();
      ctx.cancel();
      const calls: number[] = [];
      await expect(
        baml.call_int_callback_async(
          (value) => {
            calls.push(value);
            return value;
          },
          1,
          { $baml: { cancel: ctx } },
        ),
      ).rejects.toBeInstanceOf(BamlAbortError);
      expect(calls).toEqual([]);
      expect(
        await baml.call_int_callback_async((value) => {
          calls.push(value);
          return value;
        }, 2),
      ).toBe(2);
      expect(calls).toEqual([2]);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("retained_callback_uses_invocation_async_local_storage_typescript_only", async () => {
      const request = new AsyncLocalStorage<string>();
      const seen: Array<string | undefined> = [];
      const callback = async (value: number) => {
        seen.push(request.getStore());
        await nextTurn();
        expect(request.getStore()).toBe(seen[seen.length - 1]);
        return value;
      };
      const forward = await request.run("registration", () =>
        baml.make_callback_forwarder_async(
          callback as unknown as (value: number) => number,
        ),
      );
      expect(seen).toEqual([]);
      expect(
        await request.run("first invocation", () =>
          baml.call_int_callback_async(forward, 1),
        ),
      ).toBe(1);
      expect(
        await request.run("second invocation", () =>
          baml.call_int_callback_async(forward, 2),
        ),
      ).toBe(2);
      expect(seen).toEqual(["first invocation", "second invocation"]);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("cooperative_abort_allows_callback_cleanup_typescript_only", async () => {
      const ctx = CancelToken.new();
      const application = new AbortController();
      let enter!: () => void;
      const entered = new Promise<void>((resolve) => {
        enter = resolve;
      });
      let clean!: () => void;
      const cleaning = new Promise<void>((resolve) => {
        clean = resolve;
      });
      let release!: () => void;
      const cleanup = new Promise<void>((resolve) => {
        release = resolve;
      });
      let finished = false;
      let exit!: () => void;
      const exited = new Promise<void>((resolve) => {
        exit = resolve;
      });
      const callback = async (value: number) => {
        enter();
        try {
          await new Promise<void>((_resolve, reject) => {
            application.signal.addEventListener(
              "abort",
              () => reject(application.signal.reason),
              { once: true },
            );
          });
          return value;
        } finally {
          clean();
          await cleanup;
          finished = true;
          exit();
        }
      };
      const call = baml.call_int_callback_async(
        callback as unknown as (value: number) => number,
        7,
        { $baml: { cancel: ctx } },
      );
      const outcome = call.catch((error: unknown) => error);
      try {
        await entered;
        ctx.cancel();
        application.abort();
        await cleaning;
        expect(finished).toBe(false);
        release();
        expect(await outcome).toBeInstanceOf(BamlAbortError);
        await exited;
        expect(await baml.call_int_callback_async((value) => value, 8)).toBe(8);
      } finally {
        release();
        ctx.cancel();
        application.abort();
        await outcome;
      }
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("promise_all_failure_with_explicit_sibling_abort_preserves_error_typescript_only", async () => {
      // Promise.all has no TaskGroup cancellation. Application code explicitly
      // aborts its sibling BAML context and cooperative callback signal.
      const ctx = CancelToken.new();
      const application = new AbortController();
      const failure = new Error("sibling failed");
      let enter!: () => void;
      const entered = new Promise<void>((resolve) => {
        enter = resolve;
      });
      let exit!: () => void;
      const exited = new Promise<void>((resolve) => {
        exit = resolve;
      });
      const callback = async (value: number) => {
        enter();
        try {
          await new Promise<void>((_resolve, reject) => {
            application.signal.addEventListener(
              "abort",
              () => reject(application.signal.reason),
              { once: true },
            );
          });
          return value;
        } finally {
          exit();
        }
      };
      const call = baml.call_int_callback_async(
        callback as unknown as (value: number) => number,
        7,
        { $baml: { cancel: ctx } },
      );
      const outcome = call.catch((error: unknown) => error);
      const failing = (async () => {
        await entered;
        throw failure;
      })();
      try {
        await expect(Promise.all([call, failing])).rejects.toBe(failure);
      } finally {
        ctx.cancel();
        application.abort();
      }
      expect(await outcome).toBeInstanceOf(BamlAbortError);
      await exited;
      expect(await baml.call_int_callback_async((value) => value, 8)).toBe(8);
    });
  },
);
