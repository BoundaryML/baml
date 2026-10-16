import { BamlClientError, BamlRuntime, callFunctionSync, getRuntime, shutdownRuntime } from "@boundaryml/baml-bridge-web";
import { describe, expect, it } from "vitest";

describe("Web runtime shutdown", () => {
  it("ends the runtime, and repeated calls are harmless", async () => {
    const runtime = BamlRuntime.initializeRuntime(".", { "main.baml": "function answer() -> int { 42 }" });
    expect(callFunctionSync(runtime, "answer", {}).result()).toBe(42);

    await shutdownRuntime();
    await shutdownRuntime();
    expect(() => getRuntime()).toThrow(BamlClientError);
    expect(() => callFunctionSync(runtime, "answer", {})).toThrow();
  }, 30_000);
});
