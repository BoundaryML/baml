// BAML_LOG-gated delivery of BAML structured logs to the host's stderr.
// The native bridge writes to stderr at the file-descriptor level, which
// vitest cannot capture in-process, so each assertion spawns a child vitest
// run (the generated SDK is TypeScript source and needs vitest's transform)
// that executes only the env-gated child test below. Node-only: the Web and
// Workers runtimes have neither subprocesses nor a process environment for
// BAML_LOG to read.
import { describe, expect, it, type TestContext } from "vitest";
import { emit_logs } from "./baml_sdk/index.js";
import { isTestRuntime } from "./test_runtime.js";

let spawnSync: typeof import("node:child_process").spawnSync;
let fileURLToPath: typeof import("node:url").fileURLToPath;
let path: typeof import("node:path");
if (isTestRuntime("node")) {
  ({ spawnSync } = await import("node:child_process"));
  ({ fileURLToPath } = await import("node:url"));
  path = await import("node:path");
}

// Guarded: the Web and Workers runtimes may not define `process`.
const isLogSinkChild =
  typeof process !== "undefined" &&
  Boolean(process.env?.BAML_TS_LOG_SINK_CHILD);

// Runs the env-gated child test in a child vitest process with the given
// BAML_LOG value (undefined leaves it unset). Captured BAML logs land on the
// child's stderr (vitest forwards worker fd-2 writes there); the combined
// output serves diagnostics and absence checks.
function runEmitLogsChild(
  bamlLog: string | undefined,
  marker: string,
): { stderr: string; combined: string } {
  // This test file lives at <generated>/node/baml_logs.test.ts; the package
  // root with node_modules and vitest.node.config.ts is one level up.
  const generatedRoot = path.dirname(
    path.dirname(fileURLToPath(import.meta.url)),
  );
  const env: Record<string, string | undefined> = {
    ...process.env,
    BAML_TS_LOG_SINK_CHILD: "1",
    BAML_MARKER: marker,
  };
  delete env.BAML_LOG;
  if (bamlLog !== undefined) env.BAML_LOG = bamlLog;
  const child = spawnSync(
    process.execPath,
    [
      path.join(generatedRoot, "node_modules", "vitest", "vitest.mjs"),
      "run",
      "--config",
      "vitest.node.config.ts",
      path.join("node", "baml_logs.test.ts"),
      "-t",
      "baml_log_sink_child",
    ],
    { cwd: generatedRoot, encoding: "utf8", env, timeout: 180_000 },
  );
  const combined = `${child.stdout}\n${child.stderr}`;
  expect(child.status, `child vitest failed:\n${combined}`).toBe(0);
  return { stderr: child.stderr, combined };
}

// The parent/child split within the Node runtime is gated at runtime via
// ctx.skip() rather than in the runIf condition: the parity lint only
// understands plain isTestRuntime(...) runtime conditions.
describe.runIf(isTestRuntime("node"))(
  "function_calls — BAML_LOG stderr sink",
  () => {
    // SDK_PARITY_LINT(skip): requires subprocess-level SDK harness support
    it(
      "baml_log_env_var_streams_logs_to_stderr",
      { timeout: 180_000 },
      (ctx: TestContext) => {
        if (isLogSinkChild) return ctx.skip();
        const { stderr, combined } = runEmitLogsChild("info", "ts-log-marker");
        expect(stderr).toContain("[INFO] info ts-log-marker");
        expect(stderr).toContain("[WARN] warn ts-log-marker");
        expect(stderr).toContain("[ERROR] error ts-log-marker");
        // debug is below the requested info threshold; absence is checked on
        // the combined output, which is strictly stronger.
        expect(combined).not.toContain("debug ts-log-marker");
      },
    );

    // SDK_PARITY_LINT(skip): requires subprocess-level SDK harness support
    it("baml_logs_stay_off_without_baml_log", { timeout: 180_000 }, (ctx: TestContext) => {
      if (isLogSinkChild) return ctx.skip();
      const { combined } = runEmitLogsChild(undefined, "ts-quiet-marker");
      expect(combined).not.toContain("info ts-quiet-marker");
    });

    // SDK_PARITY_LINT(skip): child-process entry point for the BAML_LOG stderr tests
    it("baml_log_sink_child", (ctx: TestContext) => {
      if (!isLogSinkChild) return ctx.skip();
      const marker = process.env.BAML_MARKER ?? "ts-log-marker";
      expect(emit_logs(marker)).toBe(marker);
    });
  },
);
