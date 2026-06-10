/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import { type HandleKey } from './native.js';
/**
 * Register a JS error object and return its native `HandleKey` for the
 * `_handle: Handle(HOST_VALUE_ERROR, key)` slot of a
 * `baml.errors.HostCallable` Instance. Mints a fresh key via the Rust
 * side's shared counter (guaranteed non-zero — Rust `next_key` skips
 * `0`); stores the error keyed by the same key (recomposed as a
 * `bigint` for `Map`-key equality).
 *
 * Returns the native `HandleKey` directly so it can flow into the
 * protobufjs encoder without an intermediate `bigint→Long` conversion
 * (protobufjs reads `uint64` fields from a `{low, high}` shape, which
 * the native `HandleKey` already provides; a bare `bigint` does not
 * encode correctly through the `IInboundValue.handle.key` field).
 */
export declare function registerHostError(err: unknown): HandleKey;
/**
 * Register an arbitrary JS value as an opaque host-only value (bridge
 * generics) and return its native `HandleKey` for a
 * `Handle(HOST_VALUE_OPAQUE, key)` wire slot. Shares the error map and the
 * Rust-minted keyspace; the same release callback evicts entries when the
 * engine drops its last `HostValueArc`.
 */
export declare function registerHostOpaque(value: unknown): HandleKey;
/**
 * Look up a host-registered JS error by key. Returns `undefined` when:
 * - the key is the reserved sentinel `0n` (no real error was registered);
 * - the engine has already released the entry (last `HostValueArc` clone
 *   dropped → Rust `host_release_callback` fired → `_releaseHostError`
 *   removed the entry);
 * - the key was minted by a different Node process (cross-runtime handle).
 *
 * Callers should fall back to a metadata-built exception in those cases.
 *
 * GC/decode race: a release notification and a rehydrating decode can be
 * scheduled on the libuv loop concurrently in principle, but in practice
 * the same `HostValueArc` cannot drop *while* the engine is actively
 * emitting an outbound proto referencing its key — the outbound encode
 * holds a strong handle through proto serialization, and the release tsfn
 * isn't fired until that strong handle drops. By the time the TS decoder
 * runs `tryRehydrateFromHandle`, the only way the map entry is gone is if
 * a *prior* outbound completed and the engine has since dropped its last
 * Arc; in that case the user has already observed the original throw at
 * least once, so a second lookup-miss → metadata-fallback is acceptable.
 */
export declare function lookupHostError(key: bigint): unknown;
/**
 * Convenience for the outbound decoder: if `handle` is a `BamlHandle`
 * tagged `HOST_VALUE_ERROR`, look up the originating JS error object in
 * the registry and return it. Returns `undefined` for any other handle
 * type, a non-`BamlHandle` argument, or a key that doesn't resolve.
 *
 * Used by `decodeCallResult`'s `error` arm to rehydrate the original JS
 * exception when a BAML-thrown `baml.errors.HostCallable` propagates back
 * to the same Node process that originated it.
 */
export declare function tryRehydrateFromHandle(handle: unknown): unknown;
/**
 * Value-position rehydration (bridge generics): given a wire handle key
 * (`{low, high}`-shaped) tagged `HOST_VALUE_OPAQUE` or
 * `HOST_VALUE_CALLABLE`, return the original JS value if this process's
 * registry still holds it. Opaque entries live in the shared map here;
 * callable originals are not stored TS-side (the tsfn lives in Rust), so a
 * callable key returns `undefined` and the decoder falls back to a bare
 * `BamlHandle`.
 */
export declare function tryRehydrateHostValueByKey(key: HandleKey, handleType: number): unknown;
//# sourceMappingURL=host_error_registry.d.ts.map