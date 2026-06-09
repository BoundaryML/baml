// host_error_registry.ts — mirrors bridge_python's `register_host_error`
// + `lookup_host_value` pair.
//
// A native JS exception raised inside a user callable round-trips back to
// the *same* Node process as the *same* `Error` object (`raised === caught`
// identity), not flattened into a metadata-only `BamlError(HostCallable(...))`
// wrapper. The plumbing:
//
//   1. The TS bridge catches the JS error inside `sendHostCallableError`
//      (proto.ts) and calls `registerHostError(err)` here.
//   2. `registerHostError` mints a globally-unique key via
//      `native.mintHostErrorKey` (drawing from the shared callable+error
//      counter on the Rust side so the engine sees one keyspace), stores
//      the JS error in the local `Map<bigint, unknown>`, returns the key.
//   3. The bridge emits an `InboundValue.Class(name="baml.errors.HostCallable",
//      fields=[..., _handle: Handle(HOST_VALUE_ERROR, key)])`. The engine
//      interns an `Arc<HostValueArc>` per the same key.
//   4. When BAML propagates the throw back out to the host, the outbound
//      encoder re-emits the `_handle: Handle(HOST_VALUE_ERROR, key)`. The
//      TS decoder (proto.ts) inspects a decoded `HostCallable` instance,
//      reads `_handle.key`, calls `lookupHostError(key)` here, and re-throws
//      the original JS error.
//   5. When the engine drops its last `Arc<HostValueArc>(key)`, the Rust
//      `host_release_callback` fires the TS-installed release callback
//      (`native.registerErrorReleaseCallback`), which calls `_releaseHostError`
//      here to remove the map entry.
//
// Foreign runtimes (a different Node process, the Python bridge, etc.) see
// a `_handle` whose key doesn't resolve in their local registry; the
// decoder falls back to the metadata-bearing `BamlError(HostCallable(...))`
// wrapper. The reserved `0` is used as a sentinel by code paths that
// cannot register a real JS error (engine-internal synthetic faults like
// "no JS callable for this key"); `_releaseHostError(0n)` is a benign
// no-op since `mintHostErrorKey` never returns `0`.

import { mintHostErrorKey, registerErrorReleaseCallback, BamlHandle, type HandleKey } from './native.js';
import { baml_core } from './proto/baml_cffi.js';

const BamlHandleType = baml_core.cffi.v1.BamlHandleType;

const errorMap = new Map<bigint, unknown>();

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
function handleKeyToBigint(key: HandleKey): bigint {
    const low = BigInt(key.low >>> 0);
    const high = BigInt(key.high >>> 0);
    return (high << 32n) | low;
}

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
export function registerHostError(err: unknown): HandleKey {
    const key = mintHostErrorKey();
    errorMap.set(handleKeyToBigint(key), err);
    return key;
}

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
export function lookupHostError(key: bigint): unknown {
    return errorMap.get(key);
}

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
export function tryRehydrateFromHandle(handle: unknown): unknown {
    if (!(handle instanceof BamlHandle)) return undefined;
    if (handle.handleType !== BamlHandleType.HOST_VALUE_ERROR) return undefined;
    return lookupHostError(handleKeyToBigint(handle.key));
}

/**
 * Internal: remove the map entry for `key`. Wired at module init as the
 * Rust-side release callback. Idempotent and absent-key-safe so the same
 * callback can be invoked for *every* `HostValueArc` release (including
 * callable keys, which never have a TS-side error entry).
 */
function _releaseHostError(key: HandleKey): void {
    errorMap.delete(handleKeyToBigint(key));
}

// Install the Rust-side release callback exactly once at module load. The
// napi function is itself first-call-wins on the Rust side, so reloads
// (e.g. test harnesses) are harmless.
registerErrorReleaseCallback(_releaseHostError);
