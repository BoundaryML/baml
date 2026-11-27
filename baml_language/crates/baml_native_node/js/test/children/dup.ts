// A second copy of the runtime in the same thread (a duplicated package)
// cannot take the first one's library, and the first keeps working.
import "../baml_sdk/index.js";
import { INTERFACE_HASH, LIBRARY } from "../baml_sdk/_inlinedbaml.js";
import { add } from "../baml_sdk/bridge_test/index.js";

const second = await import(`${import.meta.resolve("@boundaryml/baml-bridge")}?second-copy`);
try {
  second.initializeNativeLibrary(LIBRARY, INTERFACE_HASH);
  console.log("loaded twice");
} catch (error) {
  console.log(`refused ${(error as Error).name}: ${(error as Error).message}`);
}
console.log(`add ${await add(1, 2)}`);
