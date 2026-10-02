/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { BamlHandle } from './native.js';
import type { BamlType, BamlTypeToken } from './wire_ty.js';
export interface InvocationOptions {
    readonly trace?: unknown;
    readonly cancel?: unknown;
    readonly timeoutMs?: number | null;
    readonly signal?: AbortSignal | null;
}
export declare class Invocation {
    readonly state: BamlHandle;
    readonly cancel: unknown;
    readonly signal: AbortSignal;
    private readonly watcher;
    private readonly notify;
    constructor(state: BamlHandle, cancel: Uint8Array);
    run<T>(body: () => T): T;
}
export declare function current(): Invocation | null;
export declare function currentContext(): unknown;
export declare function currentContextAsync(): Promise<unknown>;
export declare function withInvocation<A extends unknown[], R>(body: (active: Invocation, ...args: A) => R): (...args: A) => R;
export interface InvokeOptions {
    $baml?: InvocationOptions | null;
    $types?: Record<string, BamlType | BamlTypeToken> | null;
}
export declare function invoke(target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvokeOptions): unknown;
export declare function invokeAsync(target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvokeOptions): Promise<unknown>;
/** Link a native source to one call; retiring the waiter removes the listener. */
export declare function attachSignal(signal: AbortSignal | null | undefined, callId: bigint): () => void;
//# sourceMappingURL=invocation.d.ts.map