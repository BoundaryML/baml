import { CancelToken } from "./baml_sdk/baml/spawn/index.js";
/** TypeScript-only callback dispatch.
 * Node sync entries reject host callbacks. Promises use application-owned
 * AbortSignals; these assertions do not promise bridge-driven cancellation.
 * Native-blocking cases require an external process deadline.
 */
import "./baml_sdk/index.js";
import { BamlAbortError } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import * as baml from "./baml_sdk/host_callable_tests/index.js";
import { isTestRuntime } from "./test_runtime.js";

// Computed, ignored imports keep Node builtins out of the browser bundle.
let AsyncLocalStorage: typeof import("node:async_hooks").AsyncLocalStorage;
let createHook: typeof import("node:async_hooks").createHook;
let executionAsyncId: typeof import("node:async_hooks").executionAsyncId;
let nextTurn: typeof import("node:timers/promises").setImmediate;
let threadId: number;
if (isTestRuntime("node")) {
  ({ AsyncLocalStorage, createHook, executionAsyncId } = await import(
    /* @vite-ignore */ ["node", "async_hooks"].join(":")
  ));
  ({ setImmediate: nextTurn } = await import(
    /* @vite-ignore */ ["node", "timers/promises"].join(":")
  ));
  ({ threadId } = await import(
    /* @vite-ignore */ ["node", "worker_threads"].join(":")
  ));
}

// This overlay is also copied to browser/Workers SDKs. Node imports and Node
// environment assertions belong to the Node suite only.
describe.runIf(isTestRuntime("node"))(
  "callback_dispatch_typescript_only",
  () => {
    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejects_sync_callback_typescript_only", () => {
      const calls: number[] = [];
      const callback = (value: number) => {
        calls.push(value);
        return value + 1;
      };
      expect(() => baml.call_int_callback(callback, 6)).toThrow(
        /host callable.*async/i,
      );
      expect(calls).toEqual([]);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejects_promise_callback_typescript_only", () => {
      const calls: number[] = [];
      const callback = async (value: number) => {
        await nextTurn();
        calls.push(value);
        return value + 1;
      };
      expect(() =>
        baml.call_int_callback(
          callback as unknown as (value: number) => number,
          6,
        ),
      ).toThrow(/host callable.*async/i);
      expect(calls).toEqual([]);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejects_before_dispatch_typescript_only", () => {
      const caller = threadId;
      let ran = false;
      expect(() =>
        baml.call_int_callback((value) => {
          ran = true;
          expect(threadId).toBe(caller);
          return value;
        }, 7),
      ).toThrow(/host callable.*async/i);
      expect(ran).toBe(false); // Rejected before dispatch on Node.
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_callback_of_async_entry_runs_on_originating_loop_thread_typescript_only", async () => {
      const caller = threadId;
      expect(
        await baml.call_int_callback_async((value) => {
          expect(threadId).toBe(caller);
          return value;
        }, 7),
      ).toBe(7);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("async_callback_can_use_originating_loop_resources_typescript_only", async () => {
      const caller = threadId;
      let reply!: (value: number) => void;
      const response = new Promise<number>((resolve) => {
        reply = resolve;
      });
      const callback = async (value: number) => {
        expect(threadId).toBe(caller);
        setImmediate(() => reply(value));
        return await response;
      };
      expect(
        await baml.call_int_callback_async(
          callback as unknown as (value: number) => number,
          7,
        ),
      ).toBe(7);
      expect(await response).toBe(7);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejects_sync_callback_between_event_loop_turns_typescript_only", async () => {
      await nextTurn();
      let ran = false;
      expect(() =>
        baml.call_int_callback((value) => {
          ran = true;
          return value;
        }, 7),
      ).toThrow(/host callable.*async/i);
      expect(ran).toBe(false);
      await nextTurn();
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejects_promise_callback_between_event_loop_turns_typescript_only", async () => {
      await nextTurn();
      let ran = false;
      const callback = async (value: number) => {
        ran = true;
        await nextTurn();
        return value;
      };
      expect(() =>
        baml.call_int_callback(
          callback as unknown as (value: number) => number,
          7,
        ),
      ).toThrow(/host callable.*async/i);
      expect(ran).toBe(false);
      await nextTurn();
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejection_preserves_application_context_typescript_only", () => {
      const request = new AsyncLocalStorage<string>();
      let ran = false;
      const callback = request.run("at definition", () => (value: number) => {
        ran = true;
        expect(request.getStore()).toBe("at call");
        return value;
      });
      request.run("at call", () => {
        expect(() => baml.call_int_callback(callback, 7)).toThrow(
          /host callable.*async/i,
        );
        expect(request.getStore()).toBe("at call");
      });
      expect(ran).toBe(false); // Context inheritance is tested on the async path below.
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_callback_inherits_call_time_async_local_storage_typescript_only", async () => {
      const request = new AsyncLocalStorage<string>();
      const callback = request.run("definition", () => (value: number) => {
        expect(request.getStore()).toBe("A");
        return value;
      });
      await request.run("A", async () => {
        expect(await baml.call_int_callback_async(callback, 7)).toBe(7);
        expect(request.getStore()).toBe("A");
      });
      expect(request.getStore()).toBeUndefined();
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("promise_callback_preserves_async_local_storage_across_suspension_typescript_only", async () => {
      const request = new AsyncLocalStorage<string>();
      const callback = async (value: number) => {
        expect(request.getStore()).toBe("A");
        await nextTurn();
        expect(request.getStore()).toBe("A");
        return value;
      };
      await request.run("A", async () => {
        expect(
          await baml.call_int_callback_async(
            callback as unknown as (value: number) => number,
            7,
          ),
        ).toBe(7);
        expect(request.getStore()).toBe("A");
      });
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("concurrent_calls_isolate_async_local_storage_typescript_only", async () => {
      const request = new AsyncLocalStorage<number>();
      let arrivals = 0;
      let release!: () => void;
      const allEntered = new Promise<void>((resolve) => {
        release = resolve;
      });
      const callback = async (value: number) => {
        expect(request.getStore()).toBe(value);
        if (++arrivals === 4) release();
        await allEntered;
        expect(request.getStore()).toBe(value);
        return value;
      };
      try {
        const calls = [0, 1, 2, 3].map((value) =>
          request.run(value, () =>
            baml.call_int_callback_async(
              callback as unknown as (value: number) => number,
              value,
            ),
          ),
        );
        expect(await Promise.all(calls)).toEqual([0, 1, 2, 3]);
        expect(arrivals).toBe(4);
        expect(request.getStore()).toBeUndefined();
      } finally {
        release();
      }
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("reused_callback_uses_current_async_local_storage_typescript_only", async () => {
      const request = new AsyncLocalStorage<string>();
      const callback = async (value: number) => {
        await nextTurn();
        return `${request.getStore()}:${value}`;
      };
      expect(
        await request.run("A", () =>
          baml.call_with_callback_async(
            callback as unknown as (value: number) => string,
            1,
          ),
        ),
      ).toBe("A:1");
      expect(
        await request.run("B", () =>
          baml.call_with_callback_async(
            callback as unknown as (value: number) => string,
            2,
          ),
        ),
      ).toBe("B:2");
      expect(await request.run("B", () => callback(3))).toBe("B:3");
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("callback_reuse_across_event_loop_turns_typescript_only", async () => {
      // Node cannot close/recreate a loop within an isolate. Reuse the same
      // callback across completed async turns; isolate replacement is a gap.
      const callback = async (value: number) => {
        await nextTurn();
        return value;
      };
      expect(
        await baml.call_int_callback_async(
          callback as unknown as (value: number) => number,
          1,
        ),
      ).toBe(1);
      await nextTurn();
      expect(
        await baml.call_int_callback_async(
          callback as unknown as (value: number) => number,
          2,
        ),
      ).toBe(2);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejection_does_not_start_promise_callback_typescript_only", () => {
      const request = new AsyncLocalStorage<string>();
      let ran = false;
      const callback = async (value: number) => {
        ran = true;
        await nextTurn();
        return value;
      };
      request.run("caller", () => {
        expect(() =>
          baml.call_int_callback(
            callback as unknown as (value: number) => number,
            7,
          ),
        ).toThrow(/host callable.*async/i);
        expect(request.getStore()).toBe("caller");
      });
      expect(ran).toBe(false); // Node does not drive a bridge-owned JS event loop.
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("repeated_dispatches_each_start_with_entry_async_local_storage_typescript_only", async () => {
      const request = new AsyncLocalStorage<string>();
      const callback = async (value: number) => {
        expect(request.getStore()).toBe("caller");
        request.enterWith(String(value));
        await nextTurn();
        expect(request.getStore()).toBe(String(value));
        return String(value);
      };
      await request.run("caller", async () => {
        expect(
          await baml.call_repeatedly_async(
            callback as unknown as (value: number) => string,
            3,
          ),
        ).toEqual(["0", "1", "2"]);
        expect(request.getStore()).toBe("caller");
      });
    });
    // SDK_PARITY_LINT(skip): Node AsyncResource destruction and AsyncLocalStorage
    it("cancelled_waiter_keeps_host_resource_until_promise_exit_typescript_only", async () => {
      const request = new AsyncLocalStorage<string>();
      const ctx = CancelToken.new();
      const resources = new Set<number>();
      const destroyed = new Set<number>();
      const hook = createHook({
        init(id, type) { if (type === "BamlHostCallback") resources.add(id); },
        destroy(id) { if (resources.has(id)) destroyed.add(id); },
      }).enable();
      let markEntered!: () => void;
      const entered = new Promise<void>((resolve) => { markEntered = resolve; });
      let release!: () => void;
      const released = new Promise<void>((resolve) => { release = resolve; });
      let resourceId = -1;
      let cleanedUp = false;
      const callback = async (value: number) => {
        resourceId = executionAsyncId();
        expect(request.getStore()).toBe("original");
        markEntered();
        try {
          await released;
          return value;
        } finally {
          expect(request.getStore()).toBe("original");
          cleanedUp = true;
        }
      };
      const pending = request.run("original", () => baml.call_int_callback_async(
        callback as unknown as (value: number) => number, 6, { $baml: { cancel: ctx } },
      ));
      try {
        await entered;
        expect(resources.has(resourceId)).toBe(true);
        ctx.cancel();
        await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
        await nextTurn();
        expect(cleanedUp).toBe(false);
        expect(destroyed.has(resourceId)).toBe(false);
        release();
        await nextTurn();
        await nextTurn();
        expect(cleanedUp).toBe(true);
        expect(destroyed.has(resourceId)).toBe(true);
      } finally {
        release();
        await pending.catch(() => {});
        await nextTurn();
        hook.disable();
      }
    });
  },
);
