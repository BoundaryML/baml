// TypeScript correspondent to test_stdlib_entrypoints.py: stdlib functions of
// different `FunctionKind`s are callable as entry points (directly from the
// host), not only from inside BAML. Each gets its sync + `_async` binding.
import "./baml_sdk/index.js";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, it, expect } from "vitest";
import { now_ms, now_ms_async } from "./baml_sdk/baml/sys/index.js";
import { exists, exists_async } from "./baml_sdk/baml/fs/index.js";

// Intrinsic-only modules are not emitted at all, so a missing file is fine;
// callers only need to confirm the symbol is absent when the file exists.
function generatedSdkFile(relPath: string): string | null {
  const path = join(process.cwd(), "baml_sdk", relPath);
  if (!existsSync(path)) return null;
  return readFileSync(path, "utf8");
}

describe("function_calls — stdlib entry points", () => {
  // `baml.sys.now_ms() -> int` is a native `$rust_function`
  // (FunctionKind::Native). Calling it as an entry point should run the native
  // and return a positive millisecond timestamp, not reject with
  // `NotInvokableAsEntry`.
  it("test_native_now_ms_callable_as_entry_point", async () => {
    expect(now_ms()).toBeGreaterThan(0);
    expect(await now_ms_async()).toBeGreaterThan(0);
  });

  // `baml.fs.exists(path: string) -> bool` is a `$rust_io_function`
  // (FunctionKind::SysOp). Calling it as an entry point should run the
  // filesystem sysop and return a bool. `.` is the generated fixture
  // directory on the test host.
  it("test_sysop_fs_exists_callable_as_entry_point", async () => {
    expect(exists(".")).toBe(true);
    expect(await exists_async(".")).toBe(true);
  });

  it("test_compiler_intrinsics_are_not_emitted_as_entry_points", () => {
    const forbidden: Array<[string, string]> = [
      ["vendor/log/index.ts", "\"log.info\""],
      ["vendor/log/index.ts", "\"log.debug\""],
      ["vendor/log/index.ts", "\"log.warn\""],
      ["vendor/log/index.ts", "\"log.error\""],
      ["baml/events/index.ts", "\"baml.events.send\""],
    ];

    for (const [relPath, snippet] of forbidden) {
      const contents = generatedSdkFile(relPath);
      if (contents !== null) expect(contents).not.toContain(snippet);
    }
  });
});
