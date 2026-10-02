import type { BamlPanic } from './errors.js';
import { AsyncResource } from 'node:async_hooks';
import { _getHostCallOrigin, _discardHostCallArgs } from './native.js';

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

/** Each dispatch gets a child resource so enterWith cannot mutate the entry. */
export function runHostCallback(callId: number, args: Buffer, callback: () => void): void {
    const origin = _getHostCallOrigin(callId);
    if (origin === null || origin === undefined) {
        _discardHostCallArgs(args);
        return; // The engine cancelled this queued dispatch before it started.
    }
    const invoke = () => {
        const dispatch = new AsyncResource('BamlHostCallback', { requireManualDestroy: true });
        try {
            dispatch.runInAsyncScope(callback);
        } finally {
            dispatch.emitDestroy();
        }
    };
    const entry = callbackContexts.get(origin);
    if (entry) entry.runInAsyncScope(invoke);
    else invoke(); // Raw native callers do not install an SDK entry context.
}

export const supportsSyncStreamPulls = true;

export function handleExitPanic(code: number, _fallbackPanic: BamlPanic): never {
    process.exit(code);
}
