/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
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
/**
 * Roll back an opaque registration whose send never reached the engine: give
 * back the reference it was minted with. The map entry goes with the key's
 * last reference.
 */
export declare function releaseHostOpaque(key: HandleKey): void;
/**
 * Look up a host-registered JS value by key. Returns `undefined` when:
 * - the key is the reserved sentinel `0n` (no real value was registered);
 * - every reference to the key has been given back, so the entry is gone;
 * - the key was minted by a different Node process (cross-runtime handle).
 *
 * Callers should fall back to a metadata-built exception in those cases.
 *
 * A key the engine sends out comes with a pin that holds the engine's
 * references to it until the decode that carried it is done, so a key
 * decoded from a result of this process always resolves here.
 */
export declare function lookupHostValue(key: bigint): unknown;
/**
 * Convenience for the outbound decoder: if `handle` is a `BamlHandle`
 * tagged `HOST_VALUE_OPAQUE`, look up the originating JS value in
 * the registry and return it. Returns `undefined` for any other handle
 * type, a non-`BamlHandle` argument, or a key that doesn't resolve.
 *
 * Used by `decodeCallResult`'s `error` arm to rehydrate the original JS
 * exception when a BAML-thrown `baml.errors.HostCallable` propagates back
 * to the same Node process that originated it.
 */
export declare function tryRehydrateHostValueByKey(handle: unknown): unknown;
//# sourceMappingURL=host_value_registry.d.ts.map