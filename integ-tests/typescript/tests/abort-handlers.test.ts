import { b } from "../baml_client";
import { BamlAbortError } from "@boundaryml/baml";

describe("Abort Handlers", () => {
  it("manual cancellation", async () => {
    const controller = new AbortController();

    const promise = b.FnFailRetryExponentialDelay(5, 100, {
      signal: controller.signal,
    });

    setTimeout(() => controller.abort(), 100);

    await expect(promise).rejects.toThrow();
    // Could be BamlAbortError or another error if cancelled fast enough
  });

  it("streaming cancellation", async () => {
    const controller = new AbortController();

    const stream = b.stream.TestAbortFallbackChain("test", {
      signal: controller.signal,
    });

    setTimeout(() => {
      controller.abort();
    }, 1000);

    const values = [];
    let aborted = false;
    try {
      for await (const value of stream) {
        values.push(value);
      }
      const _ = await stream.getFinalResponse();
    } catch (e) {
      aborted = true;
      // Expected - stream should be cancelled
    }

    // Should have stopped early due to cancellation
    expect(aborted).toBe(true);
    expect(values.length).toBeLessThan(10);
  });

  it("timeout using AbortSignal.timeout", async () => {
    // Using the native AbortSignal.timeout() API
    const promise = b.FnFailRetryConstantDelay(5, 100, {
      signal: AbortSignal.timeout(200),
    });

    await expect(promise).rejects.toThrow();
  });

  it("manual timeout simulation", async () => {
    const controller = new AbortController();
    // Simulate timeout by aborting after 200ms
    setTimeout(() => controller.abort("timeout"), 200);

    const promise = b.FnFailRetryConstantDelay(5, 100, {
      signal: controller.signal,
    });

    await expect(promise).rejects.toThrow();
  });

  it("early abort check", async () => {
    const controller = new AbortController();
    controller.abort("early abort");

    await expect(
      b.ExtractName("John Doe", {
        signal: controller.signal,
      })
    ).rejects.toThrow(BamlAbortError);
  });

  it("normal operation without abort", async () => {
    const result = await b.ExtractName("My name is Alice");
    expect(typeof result).toBe("string");
    expect(result.toLowerCase()).toContain("alice");
  });
});
