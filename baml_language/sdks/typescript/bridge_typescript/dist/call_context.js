/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
/** Attach one outer call ID and return its absent-safe lifecycle owner. */
export function attachCallContext(ctx, callId) {
    const serialized = callId.toString();
    ctx?._attachCallId(serialized);
    return {
        detach() {
            ctx?._detachCallId(serialized);
        },
    };
}
//# sourceMappingURL=call_context.js.map