// TypeScript correspondent to test_stdlib_entrypoints.py: compiler intrinsics
// never surface as host-callable entry points.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { isTestRuntime } from "./test_runtime.js";

let existsSync: typeof import("node:fs").existsSync;
let readFileSync: typeof import("node:fs").readFileSync;
let join: typeof import("node:path").join;
if (isTestRuntime("node")) {
  ({ existsSync, readFileSync } = await import("node:fs"));
  ({ join } = await import("node:path"));
}

// Intrinsic-only modules are not emitted at all, so a missing file is fine;
// callers only need to confirm the symbol is absent when the file exists.
function generatedSdkFile(relPath: string): string | null {
  const path = join(process.cwd(), "baml_sdk", relPath);
  if (!existsSync(path)) return null;
  return readFileSync(path, "utf8");
}

// Inspecting generated TypeScript source requires Node's local filesystem APIs.
describe.runIf(isTestRuntime("node"))(
  "function_calls — compiler intrinsic source surface",
  () => {
    it("stdlib_entrypoints_compiler_intrinsics_are_not_emitted_as_entry_points", () => {
      const forbidden: Array<[string, string]> = [
        ["vendor/log/index.ts", '"log.info"'],
        ["vendor/log/index.ts", '"log.debug"'],
        ["vendor/log/index.ts", '"log.warn"'],
        ["vendor/log/index.ts", '"log.error"'],
        ["baml/events/index.ts", '"baml.events.send"'],
      ];

      for (const [relPath, snippet] of forbidden) {
        const contents = generatedSdkFile(relPath);
        if (contents !== null) expect(contents).not.toContain(snippet);
      }
    });
  },
);
