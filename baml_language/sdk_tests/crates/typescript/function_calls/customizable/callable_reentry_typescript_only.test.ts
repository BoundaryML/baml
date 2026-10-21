/** TypeScript-only callable reentry.
 * Node sync entries reject host callbacks. Promises use application-owned
 * AbortSignals; these assertions do not promise bridge-driven cancellation.
 * Native-blocking cases require an external process deadline.
 */
import "./baml_sdk/index.js";
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
  "callable_reentry_typescript_only",
  () => {
    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejects_callback_before_sync_reentry_typescript_only", () => {
      const calls: Array<[string, number]> = [];
      const leaf = (value: number) => {
        calls.push(["leaf", value]);
        return value + 1;
      };
      const callback = (value: number) => {
        calls.push(["outer", value]);
        return baml.call_int_callback(leaf, value);
      };
      expect(() => baml.call_int_callback(callback, 6)).toThrow(
        /host callable.*async/i,
      );
      expect(calls).toEqual([]);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejects_callback_before_async_reentry_typescript_only", () => {
      let ran = false;
      const callback = (value: number) => {
        ran = true;
        return baml.call_int_callback_async((item) => item + 1, value);
      };
      expect(() =>
        baml.call_int_callback(
          callback as unknown as (value: number) => number,
          6,
        ),
      ).toThrow(/host callable.*async/i);
      expect(ran).toBe(false); // No nested loop pumping on Node.
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("async_reentry_preserves_async_local_storage_typescript_only", async () => {
      const request = new AsyncLocalStorage<string>();
      const seen: Array<[string, number]> = [];
      const leaf = async (value: number) => {
        expect(request.getStore()).toBe("A");
        seen.push(["leaf", value]);
        await nextTurn();
        expect(request.getStore()).toBe("A");
        return value + 1;
      };
      const callback = async (value: number) => {
        expect(request.getStore()).toBe("A");
        seen.push(["outer", value]);
        return await baml.call_int_callback_async(
          leaf as unknown as (value: number) => number,
          value,
        );
      };
      await request.run("A", async () => {
        expect(
          await baml.call_int_callback_async(
            callback as unknown as (value: number) => number,
            6,
          ),
        ).toBe(7);
        expect(request.getStore()).toBe("A");
      });
      expect(seen).toEqual([
        ["outer", 6],
        ["leaf", 6],
      ]);
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("promise_callback_sync_reentry_rejects_host_callback_typescript_only", async () => {
      const seen: number[] = [];
      const callback = async (value: number) => {
        seen.push(value);
        await nextTurn();
        return baml.call_int_callback((item) => item + 1, value);
      };
      await expect(
        baml.call_int_callback_async(
          callback as unknown as (value: number) => number,
          6,
        ),
      ).rejects.toThrow(/host callable.*async/i);
      expect(seen).toEqual([6]);
      expect(await baml.call_int_callback_async((value) => value + 1, 6)).toBe(
        7,
      );
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_callback_sync_reentry_rejects_host_callback_typescript_only", async () => {
      const seen: number[] = [];
      const callback = (value: number) => {
        seen.push(value);
        return baml.call_int_callback((item) => item + 1, value);
      };
      await expect(baml.call_int_callback_async(callback, 6)).rejects.toThrow(
        /host callable.*async/i,
      );
      expect(seen).toEqual([6]);
      expect(await baml.call_int_callback_async((value) => value + 1, 6)).toBe(
        7,
      );
    });

    // SDK_PARITY_LINT(skip): Node event loop, AsyncLocalStorage, or cooperative Promise cancellation
    it("sync_entry_rejects_callback_before_recursive_reentry_typescript_only", () => {
      const seen: number[] = [];
      const callback = (value: number): number => {
        seen.push(value);
        return value === 0 ? 7 : baml.call_int_callback(callback, value - 1);
      };
      expect(() => baml.call_int_callback(callback, 3)).toThrow(
        /host callable.*async/i,
      );
      expect(seen).toEqual([]);
    });
  },
);
