/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { BamlCallContext } from './native.js';
export interface CallContextBinding {
    detach(): void;
}
/** Attach one outer call ID and return its absent-safe lifecycle owner. */
export declare function attachCallContext(ctx: BamlCallContext | undefined, callId: bigint): CallContextBinding;
//# sourceMappingURL=call_context.d.ts.map