import type { BamlPanic } from './errors.js';
import { AsyncResource, AsyncLocalStorage } from 'node:async_hooks';
import { _startHostCallExecution, _finishHostCallExecution, _discardHostCallArgs, _hostInvocationFrame, _hostCaptureRequested, _recordHostCallResult } from './native.js';

import { Invocation } from './invocation.js';
import { capture, failureOutcome, diagnostic } from './host_capture.js';
const invocationFrames = new AsyncLocalStorage<Invocation>();
export function getCurrentInvocation(): Invocation | undefined { return invocationFrames.getStore(); }
export function runWithInvocation<T>(active: Invocation, body: () => T): T { return invocationFrames.run(active, body); }
export function currentInvocationState(): string | undefined {
    const key = invocationFrames.getStore()?.state.key;
    return key === undefined ? undefined : ((BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0)).toString();
}

const hostAdoptions = new AsyncLocalStorage<{ identity?: object; consumed: boolean }>();
export function consumeHostAdoption(identity: object): boolean {
    const permit = hostAdoptions.getStore();
    if (!permit || permit.identity !== identity || permit.consumed) return false;
    permit.consumed = true;
    return true;
}

export function observeHostCallbackResult(callId: number, error: boolean, value: unknown): void {
    const execution = activeHostExecutions.get(callId);
    if (!execution) return;
    try {
        const outcome = error ? failureOutcome(value) : 'ok';
        const copied = _hostCaptureRequested(execution.lease, outcome) ? capture(value) : undefined;
        _recordHostCallResult(execution.lease, outcome, copied);
    } catch { diagnostic('host callback trace completion failed'); }
}

const callbackContexts = new Map<string, AsyncResource>();

/** Capture the SDK entry, rather than the lifetime of a registered callable. */
export function captureCallbackContext(callId: bigint): () => void {
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
const activeHostExecutions = new Map<number, {
    resource: AsyncResource;
    callback: () => void | Promise<void>;
    args: Buffer;
    lease: object;
    completion?: Promise<void>;
}>();

/** Each execution owns a child context through Promise settlement/cleanup. */
export function runHostCallback(
    callId: number, args: Buffer, callback: () => void | Promise<void>, lease?: object, markerIdentity?: object,
): void | Promise<void> {
    if (!lease) { _discardHostCallArgs(args); return; }
    const origin = _startHostCallExecution(lease);
    if (origin === null || origin === undefined) {
        _finishHostCallExecution(lease);
        return; // Cancellation won before this queued execution started.
    }
    const invoke = (): void | Promise<void> => {
        const resource = new AsyncResource('BamlHostCallback', { requireManualDestroy: true });
        const execution = { resource, callback, args, lease, completion: undefined as Promise<void> | undefined };
        activeHostExecutions.set(callId, execution);
        const finish = () => {
            activeHostExecutions.delete(callId);
            try {
                resource.emitDestroy();
            } finally {
                _finishHostCallExecution(lease);
            }
        };
        try {
            const [state, cancel] = _hostInvocationFrame(lease);
            const frame = new Invocation(state, cancel);
            const completion = resource.runInAsyncScope(() => invocationFrames.run(frame, () => hostAdoptions.run({ identity: markerIdentity, consumed: false }, callback)));
            if (completion) {
                execution.completion = completion.finally(finish);
                return execution.completion;
            }
        } catch (error) {
            finish();
            throw error;
        }
        finish();
    };
    const entry = callbackContexts.get(origin);
    if (entry) return entry.runInAsyncScope(invoke);
    return invoke(); // Raw native callers do not install an SDK entry context.
}

export const supportsSyncStreamPulls = true;

export function handleExitPanic(code: number, _fallbackPanic: BamlPanic): never {
    process.exit(code);
}
