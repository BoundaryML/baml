/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// Private implementation. Generated SDK facades provide concrete stdlib types.
import { cancelFunctionCall } from './native.js';
import { invokeTarget } from './proto.js';
export function invoke(target, args, options) {
    return invokeTarget(target, args, options, false);
}
export async function invokeAsync(target, args, options) {
    return await invokeTarget(target, args, options, true);
}
/** Link a native source to one call; retiring the waiter removes the listener. */
export function attachSignal(signal, callId) {
    if (!signal)
        return () => { };
    const abort = () => { cancelFunctionCall(callId.toString()); };
    signal.addEventListener('abort', abort, { once: true });
    if (signal.aborted)
        abort();
    return () => signal.removeEventListener('abort', abort);
}
//# sourceMappingURL=invocation.js.map