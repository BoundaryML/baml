import { flushEvents } from './native.js';
import type { BamlPanic } from './errors.js';

export const supportsSyncStreamPulls = true;

/** Test-only diagnostics (e.g. `_hostValueCount`) are opt-in per process. */
export function diagnosticsEnabled(): boolean {
    return process.env.BAML_BRIDGE_DIAGNOSTICS === '1';
}

export function handleExitPanic(code: number, _fallbackPanic: BamlPanic): never {
    flushEvents();
    process.exit(code);
}
