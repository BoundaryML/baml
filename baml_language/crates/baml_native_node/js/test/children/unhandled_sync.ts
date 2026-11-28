// An unhandled failure reported during a `_sync` call is thrown on the loop,
// as from an async call.
import "../baml_sdk/index.js";
import { fail_quietly_sync, nap } from "../baml_sdk/bridge_test/index.js";

process.on("uncaughtException", (error) => {
  console.log(`uncaught ${error.name} ${error.message}`);
});
console.log(`returned ${fail_quietly_sync()}`);
await nap(20);
