/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Private implementation. Generated SDK facades provide concrete stdlib types.
import { _invocationContext, _watchInvocationCancellation, _isInvocationCancelled, cancelFunctionCall } from './native.js';
import { decodeOutboundValue, invokeTarget } from './proto.js';
import { getCurrentInvocation, runWithInvocation } from './platform.js';
export class Invocation {
    state;
    cancel;
    signal;
    // Both JS and native owners live exactly as long as this captured frame.
    watcher;
    notify;
    constructor(state, cancel) {
        this.state = state;
        this.cancel = decodeOutboundValue(cancel);
        const controller = new AbortController();
        this.signal = controller.signal;
        this.notify = () => controller.abort();
        this.watcher = _watchInvocationCancellation(state, this.notify);
        if (_isInvocationCancelled(state))
            this.notify();
    }
    run(body) { return runWithInvocation(this, body); }
}
export function current() { return getCurrentInvocation() ?? null; }
export function currentContext() { return decodeOutboundValue(_invocationContext(current()?.state)); }
export async function currentContextAsync() { return currentContext(); }
export function withInvocation(body) {
    return (...args) => {
        const active = current();
        if (!active)
            throw new Error('withInvocation must be called by the BAML host dispatcher');
        return body(active, ...args);
    };
}
export function invoke(target, args, options) {
    return invokeTarget(target, args, options, false);
}
export async function invokeAsync(target, args, options) {
    return await invokeTarget(target, args, options, true);
}
/** Link a native source to one call; retiring the waiter removes the listener. */
export function attachSignal(signal, callId) {
    if (!signal)
        return () => { };
    const abort = () => { cancelFunctionCall(callId.toString()); };
    signal.addEventListener('abort', abort, { once: true });
    if (signal.aborted)
        abort();
    return () => signal.removeEventListener('abort', abort);
}
//# sourceMappingURL=invocation.js.map