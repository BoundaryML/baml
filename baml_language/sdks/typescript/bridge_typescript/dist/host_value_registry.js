/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// host_value_registry.ts — mirrors bridge_python's `register_host_opaque`
// + `lookup_host_value` pair.
//
// An arbitrary host JS value round-trips back to the *same* Node process as
// the *same* object (identity preserved), not flattened into a metadata-only
// wrapper. A native JS exception raised inside a user callable is the primary
// consumer: it round-trips as the *same* `Error` object (`raised === caught`
// identity), not a metadata-only `BamlError(HostCallable(...))` wrapper. The
// plumbing:
//
//   1. The TS bridge catches the JS error inside `sendHostCallableError`
//      (proto.ts) and calls `registerHostOpaque(err)` here.
//   2. `registerHostOpaque` mints a globally-unique key via
//      `native.mintHostValueKey` (drawing from the shared callable+opaque
//      counter on the Rust side so the engine sees one keyspace), stores
//      the JS value in the local `Map<bigint, unknown>`, returns the key.
//   3. The bridge emits an `InboundValue.Class(name="baml.errors.HostCallable",
//      fields=[..., _handle: Handle(HOST_VALUE_OPAQUE, key)])`. The engine
//      interns an `Arc<HostValueArc>` per the same key.
//   4. When BAML propagates the throw back out to the host, the outbound
//      encoder re-emits the `_handle: Handle(HOST_VALUE_OPAQUE, key)`. The
//      TS decoder (proto.ts) inspects a decoded `HostCallable` instance,
//      reads `_handle.key`, calls `lookupHostValue(key)` here, and re-throws
//      the original JS error.
//   5. When the engine drops its last `Arc<HostValueArc>(key)`, the Rust
//      `host_release_callback` fires the TS-installed release callback
//      (`native.registerHostValueReleaseCallback`), which calls `_releaseHostValue`
//      here to remove the map entry.
//
// Foreign runtimes (a different Node process, the Python bridge, etc.) see
// a `_handle` whose key doesn't resolve in their local registry; the
// decoder falls back to the metadata-bearing `BamlError(HostCallable(...))`
// wrapper. The reserved `0` is used as a sentinel by code paths that
// cannot register a real JS value (engine-internal synthetic faults like
// "no JS callable for this key"); `_releaseHostValue(0n)` is a benign
// no-op since `mintHostValueKey` never returns `0`.
import { mintHostValueKey, registerHostValueReleaseCallback, BamlHandle } from './native.js';
import { baml_bridge } from './proto/baml_cffi.js';
const BamlHandleType = baml_bridge.cffi.v1.BamlHandleType;
const hostValueMap = new Map();
/**
 * Convert a `HandleKey` (`{ low, high }`) to a `bigint` for use as a `Map` key.
 * Native `HandleKey` instances split a `u64` across two `i32` low/high halves
 * (signed, two's-complement). Recompose by treating each half as a 32-bit
 * unsigned value via `>>> 0`, then shift+or as `bigint`.
 *
 * The `>>> 0` coercion matters whenever either half's MSB is set: without it,
 * a negative `i32` would widen to a negative `bigint` and corrupt the
 * recomposed `u64`. Examples:
 *
 *   - `{ low: 1, high: 0 }` →  `0x1n`               (small key, no MSB set)
 *   - `{ low: -1, high: -1 }` → `0xFFFFFFFFFFFFFFFFn` (u64::MAX; both halves'
 *      MSB set — `>>> 0` reinterprets `-1` as `0xFFFFFFFF` before widening)
 *   - `{ low: 0, high: 1 }` →  `0x1_00000000n`     (2^32)
 */
function handleKeyToBigint(key) {
    const low = BigInt(key.low >>> 0);
    const high = BigInt(key.high >>> 0);
    return (high << 32n) | low;
}
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
export function registerHostOpaque(value) {
    const key = mintHostValueKey();
    hostValueMap.set(handleKeyToBigint(key), value);
    return key;
}
/** Roll back an opaque registration that was never transferred to Rust. */
export function releaseHostOpaque(key) {
    hostValueMap.delete(handleKeyToBigint(key));
}
/**
 * Look up a host-registered JS value by key. Returns `undefined` when:
 * - the key is the reserved sentinel `0n` (no real value was registered);
 * - the engine has already released the entry (last `HostValueArc` clone
 *   dropped → Rust `host_release_callback` fired → `_releaseHostValue`
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
 * runs `tryRehydrateHostValueByKey`, the only way the map entry is gone is if
 * a *prior* outbound completed and the engine has since dropped its last
 * Arc; in that case the user has already observed the original throw at
 * least once, so a second lookup-miss → metadata-fallback is acceptable.
 */
export function lookupHostValue(key) {
    return hostValueMap.get(key);
}
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
export function tryRehydrateHostValueByKey(handle) {
    if (!(handle instanceof BamlHandle))
        return undefined;
    if (handle.handleType !== BamlHandleType.HOST_VALUE_OPAQUE)
        return undefined;
    return lookupHostValue(handleKeyToBigint(handle.key));
}
/**
 * Internal: remove the map entry for `key`. Wired at module init as the
 * Rust-side release callback. Idempotent and absent-key-safe so the same
 * callback can be invoked for *every* `HostValueArc` release (including
 * callable keys, which never have a TS-side host-value entry).
 */
function _releaseHostValue(key) {
    hostValueMap.delete(handleKeyToBigint(key));
}
// Install the Rust-side release callback exactly once at module load. The
// napi function is itself first-call-wins on the Rust side, so reloads
// (e.g. test harnesses) are harmless.
registerHostValueReleaseCallback(_releaseHostValue);
//# sourceMappingURL=host_value_registry.js.map