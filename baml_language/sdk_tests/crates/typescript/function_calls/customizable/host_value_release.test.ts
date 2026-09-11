// The engine releases host values (a JS error thrown from a callback and
// carried through BAML by identity) from its GC drain, in bursts. The Node
// bridge used to forward one notification per key through a 4096-entry
// queue and dropped the rest on `QueueFull`, leaking every dropped entry
// for the life of the process. The release channel now coalesces keys and
// batches them, so a burst of any size drains completely.
import "./baml_sdk/index.js";
import { initializeRuntimeFromBytecode } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import { BYTECODE } from "./baml_sdk/_inlinedbaml.js";
import { collect_thrown_errors_async } from "./baml_sdk/host_callable_tests/index.js";
import { isTestRuntime } from "./test_runtime.js";

// Larger than the queue the old per-key channel could hold.
const BURST = 5000;

async function until(check: () => boolean, timeoutMs = 20_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!check() && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 5));
  }
  expect(check()).toBe(true);
}

describe.runIf(isTestRuntime("node"))("function_calls — host-value release channel", () => {
  it("host_callables_releases_a_burst_of_thrown_values_larger_than_a_bounded_queue", async () => {
    // `_hostValueCount` is a Node-only diagnostic gated behind this variable.
    process.env.BAML_BRIDGE_DIAGNOSTICS = "1";
    const { _hostValueCount } = (await import("@boundaryml/baml-bridge")) as unknown as {
      _hostValueCount(): number;
    };
    const baseline = _hostValueCount();

    // The fixture keeps every caught error alive until it returns, so the
    // registry is at its peak when the last callback runs.
    let peak = 0;
    const throwing = (i: number): string => {
      if (i === BURST - 1) peak = _hostValueCount();
      throw new Error(`boom ${i}`);
    };
    await expect(collect_thrown_errors_async(throwing, BURST)).resolves.toBe(BURST);
    expect(peak).toBeGreaterThanOrEqual(baseline + BURST - 1);

    // Release is GC-timed. Replacing the runtime retires the old one; its
    // final sweep releases everything it still held, in one burst.
    initializeRuntimeFromBytecode(BYTECODE);
    await until(() => _hostValueCount() === baseline);
  });
});
