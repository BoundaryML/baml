// A worker_threads worker that imports the bindings: the addon belongs to
// the main thread, so loading the library here is refused.
import { parentPort } from "node:worker_threads";

try {
  await import("../baml_sdk/index.js");
  const { add } = await import("../baml_sdk/bridge_test/index.js");
  parentPort?.postMessage(`called ${await add(1, 2)}`);
} catch (error) {
  parentPort?.postMessage(`${(error as Error).name}: ${(error as Error).message}`);
}
