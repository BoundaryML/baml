import { _discardHostCallArgs, _hostInvocationFrame, _startHostCallExecution, completeHostCall, type BamlHandle } from './native.js';
import type { BamlPanic } from './shared/errors.js';

let currentFrame: BamlHandle | undefined;
export function currentInvocationState(): string | undefined {
    const key = currentFrame?.key;
    return key ? ((BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0)).toString() : undefined;
}

export const supportsSyncStreamPulls = false;

// Web does not provide Node's AsyncLocalStorage/AsyncResource execution model.
export function captureCallbackContext(_callId: bigint): () => void {
  return () => {};
}

export function runHostCallback(callId: number, args: Uint8Array, callback: () => void | Promise<void>, execution?: object): void | Promise<void> {
  // Internal Web I/O has no inherited application frame.
  if (!execution) return callback();
  if (_startHostCallExecution(execution) === null) {
    _discardHostCallArgs(args);
    completeHostCall(callId, 1, new Uint8Array());
    return;
  }
  const previous = currentFrame;
  currentFrame = _hostInvocationFrame(execution);
  try { return callback(); }
  finally { currentFrame = previous; }
}

export function handleExitPanic(_code: number, fallbackPanic: BamlPanic): never {
  throw fallbackPanic;
}
