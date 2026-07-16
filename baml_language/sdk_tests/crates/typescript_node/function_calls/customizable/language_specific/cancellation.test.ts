import "../baml_sdk/index.js";
import {
  BamlCallContext,
  BamlCancelledError,
} from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import { SleepMs_async } from "../baml_sdk/throws_test/index.js";

describe("function_calls — TypeScript/Node-specific cancellation", () => {
  it("supports an already-aborted async call context", async () => {
    const ctx = new BamlCallContext();
    ctx.abort();

    await expect(SleepMs_async(2_000, { $ctx: ctx })).rejects.toMatchObject({
      name: "AbortError",
    });
  });

  it("surfaces a reused call context with the decoded BAML reason", async () => {
    const ctx = new BamlCallContext();
    const pending = SleepMs_async(2_000, { $ctx: ctx });
    await new Promise((resolve) => setTimeout(resolve, 50));
    ctx.abort();

    try {
      await pending;
      throw new Error("expected generated async call to reject");
    } catch (error) {
      expect((error as Error).name).toBe("AbortError");
      expect((error as { reason?: unknown }).reason).toBeInstanceOf(BamlCancelledError);
    }
  });

  it("supports an already-aborted AbortSignal", async () => {
    const controller = new AbortController();
    controller.abort();

    await expect(
      SleepMs_async(2_000, { $signal: controller.signal }),
    ).rejects.toMatchObject({ name: "AbortError" });
  });

  it("AbortSignal cancels only its call when a context is shared", async () => {
    const controller = new AbortController();
    const ctx = new BamlCallContext();
    const cancelled = SleepMs_async(2_000, {
      $signal: controller.signal,
      $ctx: ctx,
    });
    const survivor = SleepMs_async(150, { $ctx: ctx });

    await new Promise((resolve) => setTimeout(resolve, 30));
    controller.abort();

    const [cancelledResult, survivorResult] = await Promise.allSettled([
      cancelled,
      survivor,
    ]);
    expect(cancelledResult).toMatchObject({
      status: "rejected",
      reason: { name: "AbortError" },
    });
    expect(survivorResult).toEqual({ status: "fulfilled", value: null });
    expect(ctx.aborted).toBe(false);
  });
});
