// TypeScript correspondent to test_stdlib_entrypoints.py: stdlib functions of
// different `FunctionKind`s are callable as entry points (directly from the
// host), not only from inside BAML. Each gets its sync + `_async` binding.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { argv, argv_async } from "./baml_sdk/baml/sys/index.js";
import { exists, exists_async } from "./baml_sdk/baml/fs/index.js";
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

describe("function_calls — portable stdlib entry points", () => {
  // `baml.sys.argv() -> string[]` is a native `$rust_function`
  // (FunctionKind::Native). Calling it as an entry point should run the native
  // and return the argument array, not reject with `NotInvokableAsEntry`. The
  // fixture host passes no program arguments, so the array is legitimately
  // empty — the shape is what this asserts.
  it("stdlib_entrypoints_native_baml_sys_argv_is_callable_as_an_entry_point", async () => {
    expect(Array.isArray(argv())).toBe(true);
    expect(Array.isArray(await argv_async())).toBe(true);
  });
});

// The positive exists case depends on Node's local filesystem capability.
describe.runIf(isTestRuntime("node"))(
  "function_calls — Node filesystem stdlib entry points",
  () => {
    // `baml.fs.exists(path: string) -> bool` is a `$rust_io_function`
    // (FunctionKind::SysOp). Calling it as an entry point should run the
    // filesystem sysop and return a bool. `.` is the generated fixture
    // directory on the test host.
    it("stdlib_entrypoints_sysop_fs_exists_callable_as_entry_point", async () => {
      expect(exists(".")).toBe(true);
      expect(await exists_async(".")).toBe(true);
    });
  },
);

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
