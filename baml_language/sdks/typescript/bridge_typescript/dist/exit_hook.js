/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Single-registration helper for runtime shutdown and event flushing.
import { flushEvents, shutdownRuntime } from './native.js';
let installed = false;
export function installFlushOnExit() {
    if (installed)
        return;
    installed = true;
    process.once('beforeExit', async () => {
        try {
            await shutdownRuntime();
            flushEvents();
        }
        catch {
            /* ignore */
        }
    });
}
//# sourceMappingURL=exit_hook.js.map