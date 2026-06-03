// TypeScript correspondent to test_stdlib_entrypoints.py: stdlib functions of
// different `FunctionKind`s are callable as entry points (directly from the
// host), not only from inside BAML. Each gets its sync + `_async` binding.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { trunc, trunc_async } from "./baml_sdk/baml/math/index.js";
import { exists, exists_async } from "./baml_sdk/baml/fs/index.js";

describe("function_calls — stdlib entry points", () => {
  // `baml.math.trunc(value: float) -> int` is a native `$rust_function`
  // (FunctionKind::Native). Calling it as an entry point should truncate
  // toward zero and return 3, not reject with `NotInvokableAsEntry`.
  it("native baml.math.trunc is callable as an entry point", async () => {
    expect(trunc(3.7)).toBe(3);
    expect(await trunc_async(3.7)).toBe(3);
  });

  // `baml.fs.exists(path: string) -> bool` is a `$rust_io_function`
  // (FunctionKind::SysOp). Calling it as an entry point should run the
  // filesystem sysop and return a bool. `.` is the generated fixture
  // directory on the test host.
  it("sysop baml.fs.exists is callable as an entry point", async () => {
    expect(exists(".")).toBe(true);
    expect(await exists_async(".")).toBe(true);
  });
});
