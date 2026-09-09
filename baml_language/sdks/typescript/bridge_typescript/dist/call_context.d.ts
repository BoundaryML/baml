import { BamlCallContext } from './native.js';
export interface CallContextBinding {
    detach(): void;
}
/** Attach one outer call ID and return its absent-safe lifecycle owner. */
export declare function attachCallContext(ctx: BamlCallContext | undefined, callId: bigint): CallContextBinding;
//# sourceMappingURL=call_context.d.ts.map