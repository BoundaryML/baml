/**
 * Error thrown when a host callable (a JS `function`) is passed to the
 * *synchronous* call path. See {@link encodeCallArgs} for why this can't work.
 */
export declare class HostCallableSyncError extends Error {
    constructor(message: string);
}
/**
 * Encode kwargs into `CallFunctionArgs` bytes.
 *
 * `syncMode` (default false) selects the sync guard: a host callable in the
 * kwargs of a *synchronous* call rejects with {@link HostCallableSyncError}
 * before any work, rather than registering a tsfn and then hanging.
 *
 * Release tradeoff: a callable that encodes successfully is registered in the
 * host-value table and is normally released only when the engine GCs the
 * `HostClosure` it allocated and fires the C release callback (a GC-timed
 * release, drained by the engine after collection).
 * Because the Node tsfn is built with `weak::<false>` it keeps a strong libuv
 * ref, so a *leaked* registry entry can also keep the Node process from
 * exiting — which is exactly why the encode-error rollback below matters: if a
 * later kwarg fails, the engine never sees (and so never releases) the keys we
 * already registered, so we release them here.
 */
export declare function encodeCallArgs(kwargs: Record<string, unknown>, syncMode?: boolean): Buffer;
export declare function decodeCallResult(data: Buffer | Uint8Array): unknown;
//# sourceMappingURL=proto.d.ts.map