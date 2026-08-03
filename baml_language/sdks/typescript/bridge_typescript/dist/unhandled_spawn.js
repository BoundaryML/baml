/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
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