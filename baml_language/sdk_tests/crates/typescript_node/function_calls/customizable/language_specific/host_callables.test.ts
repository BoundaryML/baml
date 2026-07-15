import "../baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import {
  call_with_callback,
  call_with_callback_async,
  call_with_throwing_traceback_async,
} from "../baml_sdk/host_callable_tests/index.js";

describe("function_calls — TypeScript/Node-specific host callable guard", () => {
  it("rejects callable args on the generated sync path instead of hanging", () => {
    expect(() => call_with_callback((x: number) => `got ${x}`, 5)).toThrow(
      /host callable/i,
    );
  });

  it("round-trips throw undefined as the original rejection reason", async () => {
    const [result] = await Promise.allSettled([
      call_with_callback_async((): string => {
        throw undefined;
      }, 1),
    ]);

    expect(result.status).toBe("rejected");
    if (result.status === "rejected") {
      expect(result.reason).toBeUndefined();
    }
  });

  it("preserves a missing host traceback as null", async () => {
    await expect(
      call_with_throwing_traceback_async((): string => {
        throw undefined;
      }, 1),
    ).resolves.toBeNull();
  });
});
