import type { BamlPanic } from './shared/errors.js';

export const supportsSyncStreamPulls = false;

// Web does not provide Node's AsyncLocalStorage/AsyncResource execution model.
export function captureCallbackContext(_callId: bigint): () => void {
  return () => {};
}

export function runHostCallback(_callId: number, _args: Uint8Array, callback: () => void): void {
  callback();
}

export function handleExitPanic(_code: number, fallbackPanic: BamlPanic): never {
  throw fallbackPanic;
}
