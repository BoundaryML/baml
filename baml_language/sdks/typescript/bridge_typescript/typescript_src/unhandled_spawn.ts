import { registerUnhandledSpawnErrorCallback, type BamlEncodedResult } from './native.js';
import { decodeCallResult } from './proto.js';

export function reportUnhandledSpawnError(errorBytes: BamlEncodedResult, cancelled: boolean): void {
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
