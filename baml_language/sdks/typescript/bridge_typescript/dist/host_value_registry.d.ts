import { type HandleKey } from './native.js';
/**
 * Register an arbitrary host JS value and return its native `HandleKey` for
 * the `_handle: Handle(HOST_VALUE_OPAQUE, key)` slot of a
 * `baml.errors.HostCallable` Instance (or any future opaque-value carrier).
 * Mints a fresh key via the Rust side's shared counter (guaranteed non-zero —
 * Rust `next_key` skips `0`); stores the value keyed by the same key
 * (recomposed as a `bigint` for `Map`-key equality).
 *
 * Returns the native `HandleKey` directly so it can flow into the
 * protobufjs encoder without an intermediate `bigint→Long` conversion
 * (protobufjs reads `uint64` fields from a `{low, high}` shape, which
 * the native `HandleKey` already provides; a bare `bigint` does not
 * encode correctly through the `IInboundValue.handle.key` field).
 */
export declare function registerHostOpaque(value: unknown): HandleKey;
/** Roll back an opaque registration that was never transferred to Rust. */
export declare function releaseHostOpaque(key: HandleKey): void;
/**
 * Look up a host-registered JS value by key. Returns `undefined` when:
 * - the key is the reserved sentinel `0n` (no real value was registered);
 * - the engine has already released the entry (last `HostValueArc` clone
 *   dropped → Rust `host_release_callback` fired → `_releaseHostValues`
 *   removed the entry);
 * - the key was minted by a different Node process (cross-runtime handle).
 *
 * Callers should fall back to a metadata-built exception in those cases.
 *
 * Returned HOST_REFERENCE leases retain the registration through decoding.
 * A table key must never be interpreted as a raw host registry key.
 */
export declare function lookupHostValue(key: bigint): unknown;
/**
 * Convenience for the outbound decoder: if `handle` is a `BamlHandle`
 * tagged `HOST_REFERENCE`, look up the originating JS value in
 * the registry and return it. Returns `undefined` for any other handle
 * type, a non-`BamlHandle` argument, or a key that doesn't resolve.
 *
 * Used by `decodeCallResult`'s `error` arm to rehydrate the original JS
 * exception when a BAML-thrown `baml.errors.HostCallable` propagates back
 * to the same Node process that originated it.
 */
export declare function tryRehydrateHostValueByKey(handle: unknown): unknown;
//# sourceMappingURL=host_value_registry.d.ts.map