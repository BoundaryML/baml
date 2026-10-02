import * as bridge from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";

const packageRootExports = [
  "BamlAbortError",
  "BamlAudio",
  "BamlCancelledError",
  "BamlClientError",
  "BamlError",
  "BamlFunctionSpec",
  "BamlHandle",
  "BamlImage",
  "BamlInvalidArgumentError",
  "BamlPanic",
  "BamlPdf",
  "BamlPrompt",
  "BamlRuntime",
  "BamlStream",
  "BamlType",
  "BamlTypeMap",
  "BamlVideo",
  "FunctionResult",
  "Never",
  "TraceUsageError",
  "UNSET",
  "_ExecutionContext",
  "_currentExecutionContext",
  "_currentCancelToken",
  "_currentTraceContext",
  "_currentTraceContextAsync",
  "_instrument",
  "_invoke",
  "_invokeAsync",
  "_withExecutionContext",
  "_seedFunctionRefHandle",
  "_seedGenericMediaHandle",
  "callFunction",
  "callFunctionSync",
  "cancelFunctionCall",
  "decodeCallResult",
  "defineFunction",
  "defineInstanceFunction",
  "encodeCallArgs",
  "getBridgeRuntimeVersion",
  "getRuntime",
  "getToolchainVersion",
  "getTypeMap",
  "getVersion",
  "initializeRuntime",
  "initializeRuntimeFromBlob",
  "lowerTypeToWireTy",
  "newFunctionCall",
  "reflectType",
  "setTypeMap",
  "wrapNativeError",
] as const;

const constructors = [
  "BamlAbortError",
  "BamlAudio",
  "BamlCancelledError",
  "BamlClientError",
  "BamlError",
  "BamlFunctionSpec",
  "BamlHandle",
  "BamlImage",
  "BamlInvalidArgumentError",
  "BamlPanic",
  "BamlPdf",
  "BamlPrompt",
  "BamlRuntime",
  "BamlStream",
  "BamlType",
  "BamlTypeMap",
  "BamlVideo",
  "FunctionResult",
  "TraceUsageError",
] as const;

describe("bridge package-root parity contract", () => {
  it("bridge_surface_exports_the_same_runtime_values_in_node_browsers_and_workers", () => {
    expect(Object.keys(bridge).sort()).toEqual([...packageRootExports].sort());
  });

  it("bridge_surface_preserves_the_public_constructor_names", () => {
    for (const name of constructors) {
      const value = bridge[name];
      expect(value).toBeTypeOf("function");
      expect(value.name).toBe(name);
    }
  });
});
