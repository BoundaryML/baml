// TypeScript/Node counterpart to Python's test_errors.py. JavaScript surfaces
// BAML trace frames through Error.stack (the formatted BamlError message), and
// generated-call argument-shape mistakes ride the shared bridge validation, so
// their structured error contract matches Python too.
import "./baml_sdk/index.js";
import { spawnSync } from "node:child_process";
import {
  BamlCallContext,
  BamlCancelledError,
  BamlError,
  BamlPanic,
  callFunction,
  getRuntime,
} from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import { hello_world, raises_test, throws_test } from "./baml_sdk/index.js";
import { JsonParseError } from "./baml_sdk/baml/json/index.js";
import { InvalidArgument } from "./baml_sdk/baml/errors/index.js";
import { UserPanic } from "./baml_sdk/baml/panics/index.js";

const BAD_JSON = "{not valid json";
const TRACE_LINE = /^File "(?<file>[^"]*)", line (?<line>\d+), in (?<func>.+)$/;
const SAFE_INTEGER_ERROR = /outside JavaScript's safe-integer range/i;

function captureError(run: () => unknown): Error {
  try {
    run();
  } catch (error) {
    expect(error).toBeInstanceOf(Error);
    return error as Error;
  }
  throw new Error("expected call to throw");
}

async function captureRejection(promise: Promise<unknown>): Promise<Error> {
  try {
    await promise;
  } catch (error) {
    expect(error).toBeInstanceOf(Error);
    return error as Error;
  }
  throw new Error("expected promise to reject");
}

describe("function_calls — structured errors", () => {
  it("test_stdlib_error_surfaces_as_baml_error", () => {
    const error = captureError(() => throws_test.ParseJson(BAD_JSON));

    expect(error).toBeInstanceOf(BamlError);
    expect((error as BamlError).value).toBeInstanceOf(JsonParseError);
  });

  it("test_user_throw_surfaces_declared_instance", () => {
    const error = captureError(() => throws_test.ThrowMyError());

    expect(error).toBeInstanceOf(BamlError);
    expect((error as BamlError).value).toBeInstanceOf(throws_test.MyError);
  });

  it("test_unsafe_int_nested_in_baml_error_surfaces_explicitly", () => {
    expect(() => throws_test.ThrowUnsafeIntError()).toThrow(SAFE_INTEGER_ERROR);
  });

  it("test_unsafe_int_nested_in_baml_panic_surfaces_explicitly", () => {
    expect(() => throws_test.PanicWithUnsafeInt()).toThrow(SAFE_INTEGER_ERROR);
  });

  it("test_union_throws_preserves_class_name", () => {
    const single = captureError(() => raises_test.Reparse("x")) as BamlError;
    const union = captureError(() => raises_test.LoadDoc("x")) as BamlError;

    expect(single).toBeInstanceOf(BamlError);
    expect(union).toBeInstanceOf(BamlError);
    expect(single.className).toBe("user.raises_test.ParseError");
    expect(union.className).toBe(single.className);
    expect(union.value).toBeInstanceOf(raises_test.ParseError);
  });

  it("test_host_invalid_argument_wraps_baml_errors_invalid_argument", () => {
    const error = captureError(() =>
      (hello_world as (...args: unknown[]) => unknown)({ not_a_param: 2 }),
    ) as BamlError;

    expect(error).toBeInstanceOf(BamlError);
    expect(error.value).toBeInstanceOf(InvalidArgument);
    expect(error.className).toBe("baml.errors.InvalidArgument");
  });

  it("test_user_panic_surfaces_as_baml_panic", () => {
    const error = captureError(() => throws_test.DoPanic("user-initiated boom"));

    expect(error).toBeInstanceOf(BamlPanic);
    expect((error as BamlPanic).value).toBeInstanceOf(UserPanic);
  });

  it("test_cancellation_surfaces_as_baml_panic", async () => {
    const ctx = new BamlCallContext();
    const pending = callFunction(
      getRuntime(),
      "user.throws_test.SleepMs",
      { ms: 2000 },
      undefined,
      undefined,
      ctx,
    );

    setTimeout(() => ctx.abort(), 50);
    const error = await captureRejection(pending);

    expect(error.name).toBe("AbortError");
    expect((error as Error & { reason?: unknown }).reason).toBeInstanceOf(
      BamlCancelledError,
    );
  });

  it("test_str_is_non_empty", () => {
    const error = captureError(() => throws_test.ParseJson(BAD_JSON));

    expect(String(error).length).toBeGreaterThan(0);
  });

  it("test_baml_error_carries_baml_trace", () => {
    const error = captureError(() => throws_test.ThrowMyError()) as BamlError;

    expect(error.bamlTrace.length).toBeGreaterThan(0);
    const last = error.bamlTrace.at(-1);
    expect(last).toBeDefined();
    const match = last!.match(TRACE_LINE);
    expect(match).not.toBeNull();
    expect(match!.groups!.file.endsWith("types.baml")).toBe(true);
    expect(match!.groups!.func).toBe("user.throws_test.ThrowMyError");
    expect(Number(match!.groups!.line)).toBeGreaterThanOrEqual(1);
  });

  it("test_baml_trace_spliced_into_python_traceback", () => {
    // Node cannot synthesize native JavaScript stack frames. The bridge's
    // equivalent contract formats every BAML frame into BamlError.message,
    // which Error.stack includes ahead of the native JS frames.
    const error = captureError(() => throws_test.ParseJson(BAD_JSON)) as BamlError;
    const rendered = error.stack ?? "";

    expect(error.bamlTrace.length).toBeGreaterThan(0);
    for (const line of error.bamlTrace) {
      expect(rendered).toContain(line);
    }
    expect(rendered).toMatch(
      /File "[^"]*types\.baml", line \d+, in user\.throws_test\.ParseJson/,
    );
  });

  it("test_clean_exit_terminates_process_with_code", () => {
    // A plain Node child cannot import the generated .ts files directly on
    // every supported Node version. Initialize the same generated bytecode
    // through the packaged bridge and invoke the fixture by FQN instead.
    for (const code of [0, 7]) {
      const childScript = `
        import { readFileSync } from "node:fs";
        import {
          callFunctionSync,
          getRuntime,
          initializeRuntimeFromBytecode,
        } from "@boundaryml/baml-bridge";

        const source = readFileSync("./baml_sdk/_inlinedbaml.ts", "utf8");
        const wrapper = source.match(
          /new Uint8Array\\(\\[([\\s\\S]*?)\\]\\)/,
        );
        if (wrapper === null) {
          throw new Error("generated bytecode does not contain a Uint8Array wrapper");
        }
        const bytes = new Uint8Array(
          Array.from(wrapper[1].matchAll(/\\d+/g), (m) => Number(m[0])),
        );
        initializeRuntimeFromBytecode(bytes);
        callFunctionSync(
          getRuntime(),
          "user.throws_test.DoExit",
          { code: ${code} },
        );
        console.log("UNREACHABLE");
      `;
      const result = spawnSync(
        process.execPath,
        ["--input-type=module", "--eval", childScript],
        { cwd: process.cwd(), encoding: "utf8" },
      );

      expect(result.status, result.stderr).toBe(code);
      expect(result.stdout).not.toContain("UNREACHABLE");
    }
  });
});
