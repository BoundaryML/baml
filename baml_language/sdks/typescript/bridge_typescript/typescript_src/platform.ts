import type { BamlPanic } from './errors.js';
import { AsyncResource, AsyncLocalStorage } from 'node:async_hooks';
import { _startHostCallExecution, _finishHostCallExecution, _discardHostCallArgs, _hostInvocationFrame, type BamlHandle } from './native.js';

const invocationFrames = new AsyncLocalStorage<BamlHandle>();
export function currentInvocationState(): string | undefined {
    const key = invocationFrames.getStore()?.key;
    return key === undefined ? undefined : ((BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0)).toString();
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
    callId: number, args: Buffer, callback: () => void | Promise<void>, lease?: object,
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
            const frame = _hostInvocationFrame(lease);
            const completion = resource.runInAsyncScope(() => invocationFrames.run(frame, callback));
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
