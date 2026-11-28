// The last call ends while tasks it spawned still read the environment (I/O
// in flight, waking the library from its I/O thread) and sleep (a timer):
// the process must exit, cleanly, while they do.
import "../baml_sdk/index.js";
import { add, spawn_reader, spawn_sleeper } from "../baml_sdk/bridge_test/index.js";

console.log(`done ${await spawn_reader()} ${await spawn_sleeper()} ${await add(1, 2)}`);
