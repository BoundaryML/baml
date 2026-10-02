/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { captureCallbackContext } from './platform.js';
import { attachSignal } from './invocation.js';
import { encodedInvocation } from './proto.js';
export function attachInvocation(encoded, callId) {
    const snapshot = encodedInvocation(encoded);
    const disposeSignal = attachSignal(snapshot?.signal, callId);
    const disposeContext = captureCallbackContext(callId);
    return { detach() { try {
            disposeSignal();
            snapshot?.retained.splice(0);
        }
        finally {
            disposeContext();
        } } };
}
//# sourceMappingURL=call_context.js.map