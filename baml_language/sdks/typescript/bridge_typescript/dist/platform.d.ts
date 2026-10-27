/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import type { BamlPanic } from './errors.js';
import { ExecutionContext } from './execution_context.js';
export declare function getCurrentExecutionContext(): ExecutionContext | undefined;
export declare function runWithExecutionContext<T>(active: ExecutionContext, body: () => T): T;
export declare function currentExecutionState(): string | undefined;
export declare function consumeHostAdoption(identity: object): boolean;
export declare function observeHostCallbackResult(callId: number, error: boolean, value: unknown): void;
/** Capture the SDK entry, rather than the lifetime of a registered callable. */
export declare function captureCallbackContext(callId: bigint): () => void;
/** Each execution owns a child context through Promise settlement/cleanup. */
export declare function runHostCallback(callId: number, args: Buffer, callback: () => void | Promise<void>, lease?: object, markerIdentity?: object): void | Promise<void>;
export declare const supportsSyncStreamPulls = true;
/**
 * How the bridge binds a user function as a host callable on this platform.
 * On Node the bridge builds the threadsafe function itself, so it is handed
 * the dispatch wrapper and the host marker handle.
 */
export declare function registerCallableDispatch(dispatch: (callId: number, argsBytes: Buffer, execution?: object) => void, marker: object | undefined): [typeof dispatch, object | null];
export declare function handleExitPanic(code: number, _fallbackPanic: BamlPanic): never;
//# sourceMappingURL=platform.d.ts.map