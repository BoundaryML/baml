import { flushEvents } from './native.js';
import type { BamlPanic } from './errors.js';

export const supportsSyncStreamPulls = true;

export function handleExitPanic(code: number, _fallbackPanic: BamlPanic): never {
    flushEvents();
    process.exit(code);
}
