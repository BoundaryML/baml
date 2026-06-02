/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// exit_hook.ts — single-registration helper for flushEvents on process exit.
// Both index.ts and the CtxManager constructor used to call
// process.once('exit', …) independently; consolidate to one registration so
// `process.listenerCount('exit')` increases by exactly one per process.
Object.defineProperty(exports, "__esModule", { value: true });
exports.installFlushOnExit = installFlushOnExit;
const native_1 = require("./native");
let installed = false;
function installFlushOnExit() {
    if (installed)
        return;
    installed = true;
    process.once('exit', () => {
        try {
            (0, native_1.flushEvents)();
        }
        catch {
            /* ignore */
        }
    });
}
//# sourceMappingURL=exit_hook.js.map