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
  "getBridgeRuntimeVersion",
  "getRuntime",
  "getToolchainVersion",
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
