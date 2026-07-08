/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
// exit_hook.ts — single-registration helper for flushEvents on process exit.
// Both index.ts and the CtxManager constructor used to call
// process.once('exit', …) independently; consolidate to one registration so
// `process.listenerCount('exit')` increases by exactly one per process.
import { flushEvents } from './native.js';
let installed = false;
export function installFlushOnExit() {
    if (installed)
        return;
    installed = true;
    process.once('exit', () => {
        try {
            flushEvents();
        }
        catch {
            /* ignore */
        }
    });
}
//# sourceMappingURL=exit_hook.js.map