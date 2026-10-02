/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { AsyncResource, AsyncLocalStorage } from 'node:async_hooks';
import { _startHostCallExecution, _finishHostCallExecution, _discardHostCallArgs, _hostInvocationFrame, _hostCaptureRequested, _recordHostCallResult } from './native.js';
import { ExecutionContext } from './execution_context.js';
import { capture, failureOutcome, diagnostic } from './host_capture.js';
const executionContexts = new AsyncLocalStorage();
export function getCurrentExecutionContext() { return executionContexts.getStore(); }
export function runWithExecutionContext(active, body) { return executionContexts.run(active, body); }
export function currentExecutionState() {
    const key = executionContexts.getStore()?.state.key;
    return key === undefined ? undefined : ((BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0)).toString();
}
const hostAdoptions = new AsyncLocalStorage();
export function consumeHostAdoption(identity) {
    const permit = hostAdoptions.getStore();
    if (!permit || permit.identity !== identity || permit.consumed)
        return false;
    permit.consumed = true;
    return true;
}
export function observeHostCallbackResult(callId, error, value) {
    const execution = activeHostExecutions.get(callId);
    if (!execution)
        return;
    try {
        const outcome = error ? failureOutcome(value) : 'ok';
        const copied = _hostCaptureRequested(execution.lease, outcome) ? capture(value) : undefined;
        _recordHostCallResult(execution.lease, outcome, copied);
    }
    catch {
        diagnostic('host callback trace completion failed');
    }
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
export function runHostCallback(callId, args, callback, lease, markerIdentity) {
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
            const frame = new ExecutionContext(state, cancel);
            const completion = resource.runInAsyncScope(() => executionContexts.run(frame, () => hostAdoptions.run({ identity: markerIdentity, consumed: false }, callback)));
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