/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import { BamlRuntime, AbortController, HostSpanManager, Collector as NativeCollector, FunctionLog as NativeFunctionLog, Timing, Usage, LLMCall } from './native';
export { BamlRuntime, AbortController, BamlHandle, HostSpanManager, getVersion, getRuntime, flushEvents } from './native';
export { Timing, Usage, LLMCall } from './native';
export { takeHandleFromTable, putHandleIntoTable, _seedFunctionRefHandle, _seedGenericMediaHandle, } from './native';
export { BamlImage, BamlAudio, BamlVideo, BamlPdf } from './native';
export { BamlStream } from './stream';
export { BamlTypeMap, setTypeMap, getTypeMap } from './typemap';
export type { LazyEntry } from './typemap';
export { defineFunction } from './define_function';
export type { FunctionMode } from './define_function';
/**
 * Sentinel placeholder used by codegen Phase 2 output. Generated
 * `baml_sdk/<…>/index.ts` files emit `export const Foo = BAML_PLACEHOLDER;`
 * for every class/enum/alias/function so that imports resolve and
 * `expect(Foo).toBeDefined()`-style tests pass; Phase 4 replaces the
 * placeholders with real bindings. Frozen so accidental mutation is loud.
 */
export declare const BAML_PLACEHOLDER: any;
export { encodeCallArgs, decodeCallResult } from './proto';
export { CtxManager } from './ctx_manager';
export { BamlError, BamlInvalidArgumentError, BamlClientError, BamlCancelledError, wrapNativeError, } from './errors';
export declare class FunctionResult {
    private _value;
    constructor(value: unknown);
    result(): unknown;
    toString(): string;
}
export declare class FunctionLog {
    private _inner;
    constructor(inner: NativeFunctionLog);
    get id(): string;
    get functionName(): string;
    get timing(): Timing;
    get usage(): Usage;
    get calls(): LLMCall[];
    get tags(): Record<string, string>;
    get result(): unknown;
}
export declare class Collector {
    private _inner;
    constructor(name?: string);
    get name(): string;
    get logs(): FunctionLog[];
    get last(): FunctionLog | null;
    get usage(): Usage;
    clear(): number;
    id(functionLogId: string): FunctionLog | null;
    /** Internal: get native collector for passing to Rust */
    _native(): NativeCollector;
}
export declare function callFunctionSync(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, collectors?: Collector[], abortController?: AbortController): FunctionResult;
export declare function callFunction(rt: BamlRuntime, functionName: string, kwargs: Record<string, unknown>, ctx?: HostSpanManager, collectors?: Collector[], abortController?: AbortController): Promise<FunctionResult>;
//# sourceMappingURL=index.d.ts.map