import initWasm, * as raw from "#bridge-web-core";
import { baml_bridge } from "../dist/shared/proto/baml_cffi.js";
import {
  BamlClientError,
  BamlError,
  BamlInvalidArgumentError,
  BamlPanic,
  BamlRuntime,
  callFunction,
  callFunctionSync,
  decodeCallResult,
  flushEvents,
  getRuntime,
  getVersion,
} from "@boundaryml/baml-bridge-web";
import { beforeAll, describe, expect, it } from "vitest";

const { BamlOutboundResult } = baml_bridge.cffi.v1;

const SOURCE_A = `
function PhaseSevenValue() -> string {
  "runtime-a"
}

function PhaseSevenWait(callback: () -> string throws never) -> string {
  callback()
}
`;

const SOURCE_C = `
function PhaseSevenValue() -> string {
  "runtime-c"
}

function PhaseSevenWait(callback: () -> string throws never) -> string {
  callback()
}
`;

function encodeResult(result: baml_bridge.cffi.v1.IBamlOutboundResult): Uint8Array {
  return Uint8Array.from(BamlOutboundResult.encode(BamlOutboundResult.create(result)).finish());
}

beforeAll(async () => {
  await initWasm();
});

describe("Web runtime and setup errors", () => {
  it("reports the canonical version and keeps flushEvents harmless", () => {
    expect(getVersion()).toBe(raw.getVersion());
    expect(getVersion()).not.toBe("0.0.0-web");
    expect(() => flushEvents()).not.toThrow();
  });

  it("uses stable raw setup codes and maps them to public subclasses", () => {
    let rawError: unknown;
    try {
      raw.stageRuntimeSources(".", { "main.baml": 42 });
    } catch (error) {
      rawError = error;
    }
    expect(rawError).toBeInstanceOf(Error);
    expect((rawError as Error & { code?: string }).code).toBe("invalid_argument");

    expect(() => BamlRuntime.initializeRuntime(".", { "": SOURCE_A })).toThrow(BamlInvalidArgumentError);
    expect(() => BamlRuntime.initializeRuntimeFromBytecode(new Uint8Array([1, 2, 3]))).toThrow(/Failed to deserialize BAML bytecode/);
  });

  it("initializes from sources and replaces the singleton atomically", async () => {
    BamlRuntime.initializeRuntime(".", { "main.baml": SOURCE_A });
    expect(callFunctionSync(getRuntime(), "PhaseSevenValue", {}).result()).toBe("runtime-a");

    expect(() => BamlRuntime.initializeRuntime(".", { "broken.baml": "this is not BAML" })).toThrow(BamlClientError);
    expect(callFunctionSync(getRuntime(), "PhaseSevenValue", {}).result()).toBe("runtime-a");

    let resolveOldCall!: (value: string) => void;
    let markDispatched!: () => void;
    const dispatched = new Promise<void>((resolve) => { markDispatched = resolve; });
    const callback = (() => new Promise<string>((resolve) => {
      resolveOldCall = resolve;
      markDispatched();
    })) as unknown as () => string;
    const oldCall = callFunction(getRuntime(), "PhaseSevenWait", { callback });
    await dispatched;

    BamlRuntime.initializeRuntime(".", { "main.baml": SOURCE_C });
    resolveOldCall("old-runtime-finished");
    await expect(oldCall.then((result) => result.result())).resolves.toBe("old-runtime-finished");
    expect(callFunctionSync(getRuntime(), "PhaseSevenValue", {}).result()).toBe("runtime-c");
  }, 20_000);
});

describe("Web structured call errors", () => {
  it("maps documented error FQNs without losing detail", () => {
    const detail = {
      classValue: {
        name: "baml.errors.InvalidArgument",
        fields: [{ key: "message", value: { stringValue: "bad input" } }],
      },
    };
    let caught: unknown;
    try {
      decodeCallResult(encodeResult({ error: { value: detail, trace: ["frame"] } }));
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(BamlInvalidArgumentError);
    expect(caught).toMatchObject({ className: "baml.errors.InvalidArgument", bamlTrace: ["frame"], value: { message: "bad input" } });
  });

  it("selects client subclasses only for the documented SDK classes", () => {
    for (const className of ["baml.errors.GenericSdkError", "baml.errors.CompilationError", "baml.errors.AccessError"]) {
      expect(() => decodeCallResult(encodeResult({ error: { value: { classValue: { name: className, fields: [] } } } }))).toThrow(BamlClientError);
    }
    let caught: unknown;
    try {
      decodeCallResult(encodeResult({ error: { value: { classValue: { name: "baml.errors.TypeMismatch", fields: [] } } } }));
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(BamlError);
    expect(caught).not.toBeInstanceOf(BamlClientError);
  });

  it("turns a clean BAML exit into a Web-safe structured panic", () => {
    const value = {
      classValue: {
        name: "baml.panics.Exit",
        fields: [{ key: "message", value: { stringValue: "exit requested" } }],
      },
    };
    let caught: unknown;
    try {
      decodeCallResult(encodeResult({ panic: { value, trace: ["exit frame"], isExitPanic: true, exitCode: 7 } }));
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(BamlPanic);
    expect(caught).toMatchObject({ className: "baml.panics.Exit", bamlTrace: ["exit frame"], value: { message: "exit requested" } });
  });
});
