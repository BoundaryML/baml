/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { AsyncResource, AsyncLocalStorage } from 'node:async_hooks';
import { _startHostCallExecution, _finishHostCallExecution, _discardHostCallArgs, _hostInvocationFrame } from './native.js';
import { Invocation } from './invocation.js';
const invocationFrames = new AsyncLocalStorage();
export function getCurrentInvocation() { return invocationFrames.getStore(); }
export function runWithInvocation(active, body) { return invocationFrames.run(active, body); }
export function currentInvocationState() {
    const key = invocationFrames.getStore()?.state.key;
    return key === undefined ? undefined : ((BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0)).toString();
}
const callbackContexts = new Map();
/** Capture the SDK entry, rather than the lifetime of a registered callable. */
export function captureCallbackContext(callId) {
    const key = callId.toString();
    const entry = new AsyncResource('BamlCall', { requireManualDestroy: true });
    callbackContexts.set(key, entry);
    return () => {
        callbackContexts.delete(key);
        entry.emitDestroy();
    };
}
// Retain Promise executions even after their BAML entry removes its waiter.
// This also owns the callback/arguments and causal context through real exit.
const activeHostExecutions = new Map();
/** Each execution owns a child context through Promise settlement/cleanup. */
export function runHostCallback(callId, args, callback, lease) {
    if (!lease) {
        _discardHostCallArgs(args);
        return;
    }
    const origin = _startHostCallExecution(lease);
    if (origin === null || origin === undefined) {
        _finishHostCallExecution(lease);
        return; // Cancellation won before this queued execution started.
    }
    const invoke = () => {
        const resource = new AsyncResource('BamlHostCallback', { requireManualDestroy: true });
        const execution = { resource, callback, args, lease, completion: undefined };
        activeHostExecutions.set(callId, execution);
        const finish = () => {
            activeHostExecutions.delete(callId);
            try {
                resource.emitDestroy();
            }
            finally {
                _finishHostCallExecution(lease);
            }
        };
        try {
            const [state, cancel] = _hostInvocationFrame(lease);
            const frame = new Invocation(state, cancel);
            const completion = resource.runInAsyncScope(() => invocationFrames.run(frame, callback));
            if (completion) {
                execution.completion = completion.finally(finish);
                return execution.completion;
            }
        }
        catch (error) {
            finish();
            throw error;
        }
        finish();
    };
    const entry = callbackContexts.get(origin);
    if (entry)
        return entry.runInAsyncScope(invoke);
    return invoke(); // Raw native callers do not install an SDK entry context.
}
export const supportsSyncStreamPulls = true;
export function handleExitPanic(code, _fallbackPanic) {
    process.exit(code);
}
//# sourceMappingURL=platform.js.map