import type { BamlPanic } from './shared/errors.js';

export const supportsSyncStreamPulls = false;

/** No opt-in switch on the Web: `_hostValueCount` exists for surface parity only. */
export function diagnosticsEnabled(): boolean {
  return false;
}

export function handleExitPanic(_code: number, fallbackPanic: BamlPanic): never {
  throw fallbackPanic;
}
