import { _discardHostCallArgs, _hostInvocationFrame, _startHostCallExecution, completeHostCall, registerHostCallable, type BamlHandle } from './native.js';
import type { BamlPanic } from './shared/errors.js';

import { ExecutionContext } from './shared/execution_context.js';
let currentFrame: ExecutionContext | undefined;
export function getCurrentExecutionContext(): ExecutionContext | undefined { return currentFrame; }
export function runWithExecutionContext<T>(active: ExecutionContext, body: () => T): T {
  const previous = currentFrame;
  currentFrame = active;
  try { return body(); } finally { currentFrame = previous; }
}
export function currentExecutionState(): string | undefined {
    const key = currentFrame?.state.key;
    return key ? ((BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0)).toString() : undefined;
}

export const supportsSyncStreamPulls = false;

// Web does not provide Node's AsyncLocalStorage/AsyncResource execution model.
export function captureCallbackContext(_callId: bigint): () => void {
  return () => {};
}

export function runHostCallback(callId: number, args: Uint8Array, callback: () => void | Promise<void>, execution?: object, _markerIdentity?: object): void | Promise<void> {
  // Internal Web I/O has no inherited application frame.
  if (!execution) return callback();
  if (_startHostCallExecution(execution) === null) {
    _discardHostCallArgs(args);
    completeHostCall(callId, 1, new Uint8Array());
    return;
  }
  const previous = currentFrame;
  const [state, cancel] = _hostInvocationFrame(execution);
  currentFrame = new ExecutionContext(state, cancel);
  try { return callback(); }
  finally { currentFrame = previous; }
}

/**
 * How the bridge binds a user function as a host callable on this platform.
 * On the Web the runtime registers the frame-decoding wrapper itself and
 * answers the bridge with the callable's key as a decimal string.
 */
export function registerCallableDispatch(
    dispatch: (callId: number, argsBytes: Uint8Array, execution?: object) => void,
    marker: object | undefined,
): string {
    const key = registerHostCallable(dispatch, marker as BamlHandle | undefined);
    return (BigInt.asUintN(32, BigInt(key.high)) << 32n | BigInt.asUintN(32, BigInt(key.low))).toString();
}

export function handleExitPanic(_code: number, fallbackPanic: BamlPanic): never {
  throw fallbackPanic;
}

export function consumeHostAdoption(_identity: object): boolean { return false; }
export function observeHostCallbackResult(_callId: number, _error: boolean, _value: unknown): void {}
