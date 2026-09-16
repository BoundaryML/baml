// Node process lifetime with host callables. Registering a callable with the
// engine is ownership, not activity: it must not keep an otherwise idle Node
// process alive. Only BAML work that is still running may — including a
// background `spawn` whose callback re-enters BAML during the exit drain.
//
// Each scenario runs the public SDK in a child `node` that never calls
// `shutdownRuntime`, forces an exit, or leaves a timer armed, and asserts it
// exits 0 on its own within the timeout. The child loads the fixture's
// bytecode through the bridge directly (a plain-JS import), so it needs no
// TypeScript loader.
import "./baml_sdk/index.js";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { BYTECODE } from "./baml_sdk/_inlinedbaml.js";
import { isTestRuntime } from "./test_runtime.js";

let spawnSync: typeof import("node:child_process").spawnSync;
let mkdtempSync: typeof import("node:fs").mkdtempSync;
let rmSync: typeof import("node:fs").rmSync;
let writeFileSync: typeof import("node:fs").writeFileSync;
let tmpdir: typeof import("node:os").tmpdir;
let join: typeof import("node:path").join;
let fileURLToPath: typeof import("node:url").fileURLToPath;
if (isTestRuntime("node")) {
  ({ spawnSync } = await import("node:child_process"));
  ({ mkdtempSync, rmSync, writeFileSync } = await import("node:fs"));
  ({ tmpdir } = await import("node:os"));
  ({ join } = await import("node:path"));
  ({ fileURLToPath } = await import("node:url"));
}

describe.runIf(isTestRuntime("node"))("function_calls — Node exits on its own with host callables", () => {
  let scratch: string;
  let bytecodePath: string;

  beforeAll(() => {
    scratch = mkdtempSync(join(tmpdir(), "baml-callback-lifetime-"));
    bytecodePath = join(scratch, "fixture.bytecode");
    writeFileSync(bytecodePath, BYTECODE);
  });

  afterAll(() => {
    rmSync(scratch, { recursive: true, force: true });
  });

  function runChild(body: string) {
    const script = `
      import { readFileSync } from "node:fs";
      import { BamlRuntime, defineFunction } from "@boundaryml/baml-bridge";
      BamlRuntime.initializeRuntimeFromBytecode(readFileSync(${JSON.stringify(bytecodePath)}));
      const callWithCallback = defineFunction("user.host_callable_tests.call_with_callback", "async", ["callback", "x"]);
      const callCallbackLater = defineFunction("user.host_callable_tests.call_callback_later", "async", ["callback"]);
      const scheduleCallback = defineFunction("user.host_callable_tests.schedule_callback", "async", ["callback"]);
      ${body}
    `;
    return spawnSync(process.execPath, ["--input-type=module", "--eval", script], {
      // Resolves `@boundaryml/baml-bridge` against this fixture's node_modules.
      cwd: fileURLToPath(new URL(".", import.meta.url)),
      encoding: "utf8",
      timeout: 15_000,
    });
  }

  function expectCleanExit(child: ReturnType<typeof runChild>, ...output: string[]) {
    expect(child.error, child.stderr).toBeUndefined();
    expect(child.signal, `child was killed (${child.signal}); stderr: ${child.stderr}`).toBeNull();
    expect(child.status, child.stderr).toBe(0);
    for (const line of output) expect(child.stdout).toContain(line);
  }

  it("callback_lifetime_completed_callback_does_not_pin_an_idle_process", () => {
    const child = runChild(`console.log("value", await callWithCallback((x) => "got " + x, 5));`);
    expectCleanExit(child, "value got 5");
  });

  it("callback_lifetime_pending_call_keeps_the_process_alive_for_its_callback", () => {
    const child = runChild(`console.log("later", await callCallbackLater(() => 10));`);
    expectCleanExit(child, "later 10");
  });

  it("callback_lifetime_background_spawn_callback_reenters_baml_during_the_exit_drain", () => {
    const child = runChild(`
      await scheduleCallback(async () => {
        const value = await callWithCallback((x) => "re-" + x, 7);
        console.log("reentered", value);
        return 1;
      });
      console.log("scheduled");
    `);
    expectCleanExit(child, "scheduled", "reentered re-7");
    expect(child.stdout.indexOf("scheduled")).toBeLessThan(child.stdout.indexOf("reentered"));
  });
});
