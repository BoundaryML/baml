/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { BamlRuntime, BamlCallContext, HostSpanManager } from './native.js';
export { BamlRuntime, BamlCallContext, BamlHandle, HostSpanManager, getRuntime, getBridgeRuntimeVersion, getToolchainVersion, getVersion, flushEvents, } from './native.js';
export { _seedFunctionRefHandle, _seedGenericMediaHandle } from './native.js';
export { BamlImage, BamlAudio, BamlVideo, BamlPdf } from './native.js';
export { BamlStream } from './stream.js';
export { BamlFunctionSpec } from './function_spec.js';
export type { BamlFunctionSpecBuildRequestOptions, BamlFunctionSpecCallOptions } from './function_spec.js';
export { BamlPrompt, encodeCallArgs, decodeCallResult } from './proto.js';
export type { BamlPromptCallOptions, BamlPromptMessage } from './proto.js';
export { CtxManager } from './ctx_manager.js';
export { BamlTypeMap, setTypeMap, getTypeMap } from './typemap.js';
export { defineFunction, defineInstanceFunction, UNSET } from './define_function.js';
export type { GenericParams } from './define_function.js';
export { BamlType, Never, lowerTypeToWireTy, reflectType } from './wire_ty.js';
export type { BamlTypeMetadata, BamlTypeToken, BamlPrimitiveToken, BamlClassCtor, BamlInterfaceToken } from './wire_ty.js';
/**
 * Free-function runtime initializer used by generated `baml_sdk/index.ts`:
 * `initializeRuntime("baml_src", _inlinedbaml.FILES)`. Thin wrapper over the
 * `BamlRuntime.initializeRuntime` factory (which sets the process-global
 * singleton reachable via `getRuntime()`).
 */
export declare function initializeRuntime(srcDir: string, files: Record<string, string>): void;
/**
 * Free-function runtime initializer used by generated `baml_sdk/index.ts` when
 * codegen embeds precompiled BAML bytecode.
 */
export declare function initializeRuntimeFromBytecode(bytecode: Buffer | Uint8Array, embeddedBamlToml?: string): void;
export { BamlAbortError, BamlError, BamlInvalidArgumentError, BamlClientError, BamlCancelledError, BamlPanic, wrapNativeError, } from './errors.js';
export declare function newFunctionCall(): bigint;
export declare function cancelFunctionCall(callId: bigint): boolean;
import './unhandled_spawn.js';
export declare class FunctionResult {
    private _value;
    constructor(value: unknown);
    result(): unknown;
    toString(): string;
}
export declare function callFunctionSync(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, callCtx?: BamlCallContext): FunctionResult;
export declare function callFunction(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, callCtx?: BamlCallContext): Promise<FunctionResult>;
//# sourceMappingURL=index.d.ts.map