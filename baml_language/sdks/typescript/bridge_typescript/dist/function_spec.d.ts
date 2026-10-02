/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { BamlHandle } from './native.js';
import type { InvocationOptions } from './invocation.js';
import type { BamlPrompt } from './proto.js';
import type { BamlType } from './wire_ty.js';
export interface BamlFunctionSpecCallOptions {
    $baml?: InvocationOptions | null;
    client?: unknown;
    on_event?: unknown;
}
export interface BamlFunctionSpecBuildRequestOptions {
    $baml?: InvocationOptions | null;
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
    name(options?: {
        $baml?: InvocationOptions | null;
    }): string;
    nameAsync(options?: {
        $baml?: InvocationOptions | null;
    }): Promise<string>;
    arguments(options?: {
        $baml?: InvocationOptions | null;
    }): Record<string, unknown>;
    argumentsAsync(options?: {
        $baml?: InvocationOptions | null;
    }): Promise<Record<string, unknown>>;
    outputType(options?: {
        $baml?: InvocationOptions | null;
    }): BamlType;
    outputTypeAsync(options?: {
        $baml?: InvocationOptions | null;
    }): Promise<BamlType>;
    prompt(options?: {
        $baml?: InvocationOptions | null;
    }): BamlPrompt;
    promptAsync(options?: {
        $baml?: InvocationOptions | null;
    }): Promise<BamlPrompt>;
    tools(options?: {
        $baml?: InvocationOptions | null;
    }): unknown;
    toolsAsync(options?: {
        $baml?: InvocationOptions | null;
    }): Promise<unknown>;
    clientId(options?: {
        $baml?: InvocationOptions | null;
    }): string;
    clientIdAsync(options?: {
        $baml?: InvocationOptions | null;
    }): Promise<string>;
    buildRequest(options?: BamlFunctionSpecBuildRequestOptions): unknown;
    buildRequestAsync(options?: BamlFunctionSpecBuildRequestOptions): Promise<unknown>;
    parse(json: string, options?: {
        $baml?: InvocationOptions | null;
    }): TOut;
    parseAsync(json: string, options?: {
        $baml?: InvocationOptions | null;
    }): Promise<TOut>;
    call(options?: BamlFunctionSpecCallOptions): TOut;
    callAsync(options?: BamlFunctionSpecCallOptions): Promise<TOut>;
    private _callSync;
    private _callAsync;
    toString(): string;
}
//# sourceMappingURL=function_spec.d.ts.map