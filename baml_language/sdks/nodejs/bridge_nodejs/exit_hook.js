/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// exit_hook.ts — single-registration helper for flushEvents on process exit.
//
// Both index.ts and CtxManager constructor used to call process.once('exit',…)
// independently. This helper de-duplicates the registration so the hook is
// installed at most once per process regardless of how the module graph loads.
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
            // ignore
        }
    });
}
//# sourceMappingURL=exit_hook.js.map