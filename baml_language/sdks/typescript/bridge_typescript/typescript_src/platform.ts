import type { BamlPanic } from './errors.js';

export const supportsSyncStreamPulls = true;

export function handleExitPanic(code: number, _fallbackPanic: BamlPanic): never {
    process.exit(code);
}
