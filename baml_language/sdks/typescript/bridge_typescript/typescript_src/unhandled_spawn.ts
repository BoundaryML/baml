import { registerUnhandledSpawnErrorCallback } from './native.js';
import { decodeCallResult } from './proto.js';

export function reportUnhandledSpawnError(errorBytes: Buffer, cancelled: boolean): void {
    try {
        decodeCallResult(errorBytes);
    } catch (error) {
        if (cancelled) {
            console.error(error);
            return;
        }
        throw error;
    }
}

registerUnhandledSpawnErrorCallback((errorBytes, cancelled) => {
    queueMicrotask(() => reportUnhandledSpawnError(errorBytes, cancelled));
});
