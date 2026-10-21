// Private retained execution context; independent of execution lifetime.
import { BamlHandle, _invocationContext, _watchInvocationCancellation, _isInvocationCancelled } from './native.js';
import { decodeOutboundValue } from './proto.js';
import { getCurrentExecutionContext, runWithExecutionContext } from './platform.js';

export class ExecutionContext {
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

    run<T>(body: () => T): T { return runWithExecutionContext(this, body); }
}

export function current(): ExecutionContext | null { return getCurrentExecutionContext() ?? null; }
export function currentContext(): unknown { return decodeOutboundValue(_invocationContext(current()?.state)); }
export async function currentContextAsync(): Promise<unknown> { return currentContext(); }
export function withExecutionContext<A extends unknown[], R>(body: (active: ExecutionContext, ...args: A) => R): (...args: A) => R {
    return (...args) => {
        const active = current();
        if (!active) throw new Error('withExecutionContext must be called by the BAML host dispatcher');
        return body(active, ...args);
    };
}

/** Returns the effective token, or null outside a BAML execution context. */
export function currentCancelToken(): unknown | null { return current()?.cancel ?? null; }
