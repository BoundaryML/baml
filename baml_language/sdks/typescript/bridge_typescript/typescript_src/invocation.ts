// Private implementation. Generated SDK facades provide concrete stdlib types.
import { BamlHandle, _invocationContext, _watchInvocationCancellation, _isInvocationCancelled, cancelFunctionCall } from './native.js';
import { decodeOutboundValue, invokeTarget } from './proto.js';
import { getCurrentInvocation, runWithInvocation } from './platform.js';
import type { BamlType, BamlTypeToken } from './wire_ty.js';

export interface InvocationOptions {
    readonly trace?: unknown;
    readonly cancel?: unknown;
    readonly timeoutMs?: number | null;
    readonly signal?: AbortSignal | null;
}

export class Invocation {
    readonly state: BamlHandle;
    readonly cancel: unknown;
    readonly signal: AbortSignal;
    // Both JS and native owners live exactly as long as this captured frame.
    private readonly watcher: object;
    private readonly notify: () => void;

    constructor(state: BamlHandle, cancel: Uint8Array) {
        this.state = state;
        this.cancel = decodeOutboundValue(cancel);
        const controller = new AbortController();
        this.signal = controller.signal;
        this.notify = () => controller.abort();
        this.watcher = _watchInvocationCancellation(state, this.notify);
        if (_isInvocationCancelled(state)) this.notify();
    }

    run<T>(body: () => T): T { return runWithInvocation(this, body); }
}

export function current(): Invocation | null { return getCurrentInvocation() ?? null; }
export function currentContext(): unknown { return decodeOutboundValue(_invocationContext(current()?.state)); }
export async function currentContextAsync(): Promise<unknown> { return currentContext(); }
export function withInvocation<A extends unknown[], R>(body: (active: Invocation, ...args: A) => R): (...args: A) => R {
    return (...args) => {
        const active = current();
        if (!active) throw new Error('withInvocation must be called by the BAML host dispatcher');
        return body(active, ...args);
    };
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
