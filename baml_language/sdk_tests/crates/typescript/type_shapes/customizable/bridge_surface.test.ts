import * as bridge from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";

const packageRootExports = [
  "BamlAbortError",
  "BamlAudio",
  "BamlCallContext",
  "BamlCancelledError",
  "BamlClientError",
  "BamlError",
  "BamlHandle",
  "BamlImage",
  "BamlInvalidArgumentError",
  "BamlPanic",
  "BamlPdf",
  "BamlRuntime",
  "BamlStream",
  "BamlTypeMap",
  "BamlVideo",
  "Collector",
  "CtxManager",
  "FunctionLog",
  "FunctionResult",
  "HostSpanManager",
  "Never",
  "Timing",
  "UNSET",
  "Usage",
  "_seedFunctionRefHandle",
  "_seedGenericMediaHandle",
  "callFunction",
  "callFunctionSync",
  "cancelFunctionCall",
  "decodeCallResult",
  "defineFunction",
  "defineInstanceFunction",
  "encodeCallArgs",
  "flushEvents",
  "getRuntime",
  "getTypeMap",
  "getVersion",
  "initializeRuntime",
  "initializeRuntimeFromBytecode",
  "lowerTypeToWireTy",
  "newFunctionCall",
  "setTypeMap",
  "wrapNativeError",
] as const;

const constructors = [
  "BamlAbortError",
  "BamlAudio",
  "BamlCallContext",
  "BamlCancelledError",
  "BamlClientError",
  "BamlError",
  "BamlHandle",
  "BamlImage",
  "BamlInvalidArgumentError",
  "BamlPanic",
  "BamlPdf",
  "BamlRuntime",
  "BamlStream",
  "BamlTypeMap",
  "BamlVideo",
  "Collector",
  "CtxManager",
  "FunctionLog",
  "FunctionResult",
  "HostSpanManager",
  "Timing",
] as const;

describe("bridge package-root parity contract", () => {
  it("exports the same runtime values in Node, browsers, and Workers", () => {
    expect(Object.keys(bridge).sort()).toEqual([...packageRootExports].sort());
  });

  it("preserves the public constructor names", () => {
    for (const name of constructors) {
      const value = bridge[name];
      expect(value).toBeTypeOf("function");
      expect(value.name).toBe(name);
    }
  });
});
