/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { flushEvents } from './native.js';
export const supportsSyncStreamPulls = true;
/** Test-only diagnostics (e.g. `_hostValueCount`) are opt-in per process. */
export function diagnosticsEnabled() {
    return process.env.BAML_BRIDGE_DIAGNOSTICS === '1';
}
export function handleExitPanic(code, _fallbackPanic) {
    flushEvents();
    process.exit(code);
}
//# sourceMappingURL=platform.js.map