import { BamlRuntime, callFunctionSync } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import { hello_world } from "./baml_sdk/index.js";

function runtimeSource(value: string): string {
  return `
function RuntimeRegistryValue() -> string {
  "${value}"
}
`;
}

describe("BamlRuntime registry", () => {
  it("runtime_instances_remain_independent", () => {
    const runtimeA = BamlRuntime.initializeRuntime(".", {
      "runtime_a.baml": runtimeSource("runtime-a"),
    });
    const runtimeB = BamlRuntime.initializeRuntime(".", {
      "runtime_b.baml": runtimeSource("runtime-b"),
    });

    expect(runtimeA.runtimeKey).not.toBe(runtimeB.runtimeKey);
    expect(callFunctionSync(runtimeA, "RuntimeRegistryValue", {}).result()).toBe(
      "runtime-a",
    );
    expect(callFunctionSync(runtimeB, "RuntimeRegistryValue", {}).result()).toBe(
      "runtime-b",
    );
    expect(callFunctionSync(runtimeA, "RuntimeRegistryValue", {}).result()).toBe(
      "runtime-a",
    );
  }, 15_000);

  it("generated_sdk_keeps_using_runtime_zero", () => {
    BamlRuntime.initializeRuntime(".", {
      "dynamic.baml": runtimeSource("dynamic"),
    });

    expect(hello_world()).toBe("hello world");
  });
});
