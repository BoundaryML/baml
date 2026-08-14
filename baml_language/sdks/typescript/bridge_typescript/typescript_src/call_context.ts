import { BamlCallContext } from './native.js';

export interface CallContextBinding {
    detach(): void;
}

/** Attach one outer call ID and return its absent-safe lifecycle owner. */
export function attachCallContext(
    ctx: BamlCallContext | undefined,
    callId: bigint,
): CallContextBinding {
    const serialized = callId.toString();
    ctx?._attachCallId(serialized);
    return {
        detach() {
            ctx?._detachCallId(serialized);
        },
    };
}
