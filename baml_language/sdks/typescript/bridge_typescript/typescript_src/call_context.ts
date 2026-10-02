import { captureCallbackContext } from './platform.js';
import { attachSignal } from './invocation.js';
import { encodedInvocation } from './proto.js';

export interface CallContextBinding { detach(): void; }
export function attachInvocation(encoded: Uint8Array, callId: bigint): CallContextBinding {
    const snapshot = encodedInvocation(encoded);
    const disposeSignal = attachSignal(snapshot?.signal, callId);
    const disposeContext = captureCallbackContext(callId);
    return { detach() { try { disposeSignal(); snapshot?.retained.splice(0); } finally { disposeContext(); } } };
}
