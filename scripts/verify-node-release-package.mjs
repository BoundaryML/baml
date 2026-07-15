import { performance } from "node:perf_hooks";

const expectedVersion = process.env.EXPECTED_VERSION;
const coldImportMaxMs = Number(process.env.COLD_IMPORT_MAX_MS);
const versionCallsMaxMs = Number(process.env.VERSION_CALLS_MAX_MS);

if (
  !expectedVersion ||
  !Number.isFinite(coldImportMaxMs) ||
  !Number.isFinite(versionCallsMaxMs)
) {
  throw new Error("release smoke budgets and EXPECTED_VERSION must be set");
}

const importStart = performance.now();
const bridge = await import("@boundaryml/baml-bridge");
const coldImportMs = performance.now() - importStart;
const actualVersion = bridge.getVersion();
if (actualVersion !== expectedVersion) {
  throw new Error(
    `native version mismatch: expected ${expectedVersion}, got ${actualVersion}`,
  );
}
if (coldImportMs > coldImportMaxMs) {
  throw new Error(
    `cold import ${coldImportMs.toFixed(1)}ms exceeds ${coldImportMaxMs}ms`,
  );
}

// Exercise the same initialization + generated-call factory path used by a
// generated SDK. This catches packages that can load the native addon but
// cannot compile or execute ordinary, non-LLM BAML code.
bridge.initializeRuntime(".", {
  "release_smoke.baml": `
function add_numbers(a: int, b: int) -> int {
  a + b
}
`,
});
const addNumbers = bridge.defineFunction("user.add_numbers", "sync", [
  "a",
  "b",
]);
const sum = addNumbers(19, 23);
if (sum !== 42) {
  throw new Error(`pure BAML sync call returned ${String(sum)} instead of 42`);
}
const addNumbersAsync = bridge.defineFunction("user.add_numbers", "async", [
  "a",
  "b",
]);
const asyncSum = await addNumbersAsync(20, 22);
if (asyncSum !== 42) {
  throw new Error(
    `pure BAML async call returned ${String(asyncSum)} instead of 42`,
  );
}

const callsStart = performance.now();
for (let i = 0; i < 10_000; i += 1) {
  if (bridge.getVersion() !== expectedVersion)
    throw new Error("unstable native version result");
}
const versionCallsMs = performance.now() - callsStart;
if (versionCallsMs > versionCallsMaxMs) {
  throw new Error(
    `10,000 boundary calls took ${versionCallsMs.toFixed(1)}ms; limit is ${versionCallsMaxMs}ms`,
  );
}

console.log(
  JSON.stringify({
    actualVersion,
    coldImportMs,
    versionCallsMs,
    sum,
    asyncSum,
  }),
);
