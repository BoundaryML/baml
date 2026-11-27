// The script's last call ends while a BAML task it spawned still sleeps for
// ten seconds: Node must exit right away.
import "../baml_sdk/index.js";
import { add, env_or, nap, spawn_sleeper } from "../baml_sdk/bridge_test/index.js";

const [sum] = await Promise.all([add(1, 2), nap(20), env_or("PATH", "unset"), spawn_sleeper()]);
console.log(`done ${sum}`);
