import { registerUnhandledSpawnErrorCallback } from './native.js';
import { decodeCallResult } from './proto.js';
export function reportUnhandledSpawnError(errorBytes, cancelled) {
    try {
        decodeCallResult(errorBytes);
    }
    catch (error) {
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
//# sourceMappingURL=unhandled_spawn.js.map