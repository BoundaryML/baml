// A task a call spawned fails and nothing awaits it: as with today's bridge,
// the error is thrown on Node's loop.
import "../baml_sdk/index.js";
import { fail_quietly, nap } from "../baml_sdk/bridge_test/index.js";

process.on("uncaughtException", (error) => {
  console.log(`uncaught ${error.name} ${error.message}`);
});
console.log(`returned ${await fail_quietly()}`);
await nap(20);
