import { BamlCallContext } from './native.js';
import { captureCallbackContext } from './platform.js';

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
    const disposeCallbackContext = captureCallbackContext(callId);
    return {
        detach() {
            try {
                ctx?._detachCallId(serialized);
            } finally {
                disposeCallbackContext();
            }
        },
    };
}
