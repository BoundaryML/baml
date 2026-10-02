/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import type { BamlPanic } from './errors.js';
import { Invocation } from './invocation.js';
export declare function getCurrentInvocation(): Invocation | undefined;
export declare function runWithInvocation<T>(active: Invocation, body: () => T): T;
export declare function currentInvocationState(): string | undefined;
export declare function consumeHostAdoption(identity: object): boolean;
export declare function observeHostCallbackResult(callId: number, error: boolean, value: unknown): void;
/** Capture the SDK entry, rather than the lifetime of a registered callable. */
export declare function captureCallbackContext(callId: bigint): () => void;
/** Each execution owns a child context through Promise settlement/cleanup. */
export declare function runHostCallback(callId: number, args: Buffer, callback: () => void | Promise<void>, lease?: object, markerIdentity?: object): void | Promise<void>;
export declare const supportsSyncStreamPulls = true;
export declare function handleExitPanic(code: number, _fallbackPanic: BamlPanic): never;
//# sourceMappingURL=platform.d.ts.map