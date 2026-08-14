import * as generatedSdk from "./baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import { isTestRuntime, testRuntime } from "./test_runtime.js";

describe("generated SDK runtime selection", () => {
  it("runtime_imports_the_generated_sdk_in_the_configured_runtime", () => {
    expect(generatedSdk).toBeDefined();
    expect(["node", "web", "workers"]).toContain(testRuntime);
  });
});

describe.runIf(isTestRuntime("node"))("Node runtime selection", () => {
  it("runtime_executes_the_generated_sdk_in_node", () => {
    expect(process.release.name).toBe("node");
    expect(typeof (globalThis as { document?: unknown }).document).toBe(
      "undefined",
    );
  });
});

describe.runIf(isTestRuntime("web"))("browser runtime selection", () => {
  it("runtime_executes_the_generated_sdk_in_a_browser", () => {
    const browserGlobal = globalThis as unknown as {
      window: unknown;
      document: { createElement(tagName: string): object };
      HTMLDivElement: new () => object;
    };
    expect(browserGlobal.window).toBe(globalThis);
    expect(browserGlobal.document.createElement("div")).toBeInstanceOf(
      browserGlobal.HTMLDivElement,
    );
  });
});

describe.runIf(isTestRuntime("workers"))("Workers runtime selection", () => {
  it("runtime_executes_the_generated_sdk_in_workerd", () => {
    const workerGlobal = globalThis as typeof globalThis & {
      document?: unknown;
      WebSocketPair?: unknown;
    };
    expect(workerGlobal.document).toBeUndefined();
    expect(workerGlobal.WebSocketPair).toBeTypeOf("function");
  });
});
