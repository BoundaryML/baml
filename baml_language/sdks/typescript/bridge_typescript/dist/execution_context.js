/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Private retained execution context; independent of execution lifetime.
import { _invocationContext, _watchInvocationCancellation, _isInvocationCancelled } from './native.js';
import { decodeOutboundValue } from './proto.js';
import { getCurrentExecutionContext, runWithExecutionContext } from './platform.js';
export class ExecutionContext {
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
    run(body) { return runWithExecutionContext(this, body); }
}
export function current() { return getCurrentExecutionContext() ?? null; }
export function currentContext() { return decodeOutboundValue(_invocationContext(current()?.state)); }
export async function currentContextAsync() { return currentContext(); }
export function withExecutionContext(body) {
    return (...args) => {
        const active = current();
        if (!active)
            throw new Error('withExecutionContext must be called by the BAML host dispatcher');
        return body(active, ...args);
    };
}
/** Returns the effective token, or null outside a BAML execution context. */
export function currentCancelToken() { return current()?.cancel ?? null; }
//# sourceMappingURL=execution_context.js.map