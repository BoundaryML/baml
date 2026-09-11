/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Single-registration helper for runtime shutdown and event flushing.
import { flushEvents, shutdownRuntime, _waitForRuntimeIdle } from './native.js';
let installed = false;
export function installFlushOnExit() {
    if (installed)
        return;
    installed = true;
    process.once('beforeExit', async () => {
        try {
            // A registered host callable is ownership, not activity, and no
            // longer keeps the event loop alive on its own. This pending
            // promise does: it holds Node open until already-started BAML
            // work — including a background `spawn` that calls back into JS,
            // which may itself re-enter BAML — has settled, without closing
            // admission for that re-entry.
            await _waitForRuntimeIdle();
            await shutdownRuntime();
            flushEvents();
        }
        catch {
            /* ignore */
        }
    });
}
//# sourceMappingURL=exit_hook.js.map