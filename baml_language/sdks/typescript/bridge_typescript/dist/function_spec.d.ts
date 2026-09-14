/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { BamlHandle } from './native.js';
import type { BamlPrompt } from './proto.js';
import type { BamlType } from './wire_ty.js';
export interface BamlFunctionSpecCallOptions {
    client?: unknown;
    on_event?: unknown;
}
export interface BamlFunctionSpecBuildRequestOptions {
    client?: unknown;
}
/** An opaque, bound LLM recipe owned by the engine that created it. */
export declare class BamlFunctionSpec<TOut> {
    private readonly handle;
    constructor(handle: BamlHandle);
    /** Internal: construct a FunctionSpec proxy from a tagged heap handle. */
    static _fromHandle<TOut>(handle: BamlHandle, _classFqn: string): BamlFunctionSpec<TOut>;
    /** Internal: expose the inner handle for inbound encoding. */
    _toHandle(): BamlHandle;
    name(): string;
    nameAsync(): Promise<string>;
    arguments(): Record<string, unknown>;
    argumentsAsync(): Promise<Record<string, unknown>>;
    outputType(): BamlType;
    outputTypeAsync(): Promise<BamlType>;
    prompt(): BamlPrompt;
    promptAsync(): Promise<BamlPrompt>;
    tools(): unknown;
    toolsAsync(): Promise<unknown>;
    clientId(): string;
    clientIdAsync(): Promise<string>;
    buildRequest(options?: BamlFunctionSpecBuildRequestOptions): unknown;
    buildRequestAsync(options?: BamlFunctionSpecBuildRequestOptions): Promise<unknown>;
    parse(json: string): TOut;
    parseAsync(json: string): Promise<TOut>;
    call(options?: BamlFunctionSpecCallOptions): TOut;
    callAsync(options?: BamlFunctionSpecCallOptions): Promise<TOut>;
    private _callSync;
    private _callAsync;
    toString(): string;
}
//# sourceMappingURL=function_spec.d.ts.map