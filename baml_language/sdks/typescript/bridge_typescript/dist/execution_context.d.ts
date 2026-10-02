/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { BamlHandle } from './native.js';
export declare class ExecutionContext {
    readonly state: BamlHandle;
    readonly cancel: unknown;
    readonly signal: AbortSignal;
    private readonly watcher;
    private readonly notify;
    constructor(state: BamlHandle, cancel: Uint8Array);
    run<T>(body: () => T): T;
}
export declare function current(): ExecutionContext | null;
export declare function currentContext(): unknown;
export declare function currentContextAsync(): Promise<unknown>;
export declare function withExecutionContext<A extends unknown[], R>(body: (active: ExecutionContext, ...args: A) => R): (...args: A) => R;
/** Returns the effective token, or null outside a BAML execution context. */
export declare function currentCancelToken(): unknown | null;
//# sourceMappingURL=execution_context.d.ts.map