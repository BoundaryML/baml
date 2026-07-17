import type { BamlPanic } from './shared/errors.js';

export const supportsSyncStreamPulls = false;

export function handleExitPanic(_code: number, fallbackPanic: BamlPanic): never {
  throw fallbackPanic;
}
