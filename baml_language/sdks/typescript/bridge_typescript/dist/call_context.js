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