/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import type { BamlType, BamlTypeToken } from './wire_ty.js';
export interface InvocationOptions {
    readonly trace?: unknown;
    readonly cancel?: unknown;
    readonly timeoutMs?: number | null;
    readonly signal?: AbortSignal | null;
}
export interface InvokeOptions {
    $baml?: InvocationOptions | null;
    $types?: Record<string, BamlType | BamlTypeToken> | null;
}
export declare function invoke(target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvokeOptions): unknown;
export declare function invokeAsync(target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvokeOptions): Promise<unknown>;
/** Link a native source to one call; retiring the waiter removes the listener. */
export declare function attachSignal(signal: AbortSignal | null | undefined, callId: bigint): () => void;
//# sourceMappingURL=invocation.d.ts.map