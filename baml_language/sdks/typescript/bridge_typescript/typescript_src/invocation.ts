// Private implementation. Generated SDK facades provide concrete stdlib types.
import { cancelFunctionCall } from './native.js';
import { invokeTarget } from './proto.js';
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
export function invoke(target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvokeOptions): unknown {
    return invokeTarget(target, args, options, false);
}
export async function invokeAsync(target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options?: InvokeOptions): Promise<unknown> {
    return await invokeTarget(target, args, options, true);
}

/** Link a native source to one call; retiring the waiter removes the listener. */
export function attachSignal(signal: AbortSignal | null | undefined, callId: bigint): () => void {
    if (!signal) return () => {};
    const abort = () => { cancelFunctionCall(callId.toString()); };
    signal.addEventListener('abort', abort, { once: true });
    if (signal.aborted) abort();
    return () => signal.removeEventListener('abort', abort);
}
