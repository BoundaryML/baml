/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// proto.ts — mirrors bridge_python/python_src/baml_py/proto.py
//
// Encodes TS objects → CallFunctionArgs protobuf bytes (for sending to Rust)
// Decodes BamlOutboundValue protobuf bytes → TS objects (for receiving from Rust)
Object.defineProperty(exports, "__esModule", { value: true });
exports.HostCallableSyncError = void 0;
exports.encodeCallArgs = encodeCallArgs;
exports.decodeCallResult = decodeCallResult;
const baml_cffi_1 = require("./proto/baml_cffi");
const native_1 = require("./native");
const CallFunctionArgs = baml_cffi_1.baml_core.cffi.v1.CallFunctionArgs;
const BamlOutboundValue = baml_cffi_1.baml_core.cffi.v1.BamlOutboundValue;
const InboundValue = baml_cffi_1.baml_core.cffi.v1.InboundValue;
const HostCallableError = baml_cffi_1.baml_core.cffi.v1.HostCallableError;
const HostCallableErrorCategory = baml_cffi_1.baml_core.cffi.v1.HostCallableErrorCategory;
const BamlHandleType = baml_cffi_1.baml_core.cffi.v1.BamlHandleType;
// ─── Inbound (TS → Rust) ───
function isPlainObject(value) {
    if (value == null || typeof value !== 'object')
        return false;
    const proto = Object.getPrototypeOf(value);
    return proto === Object.prototype || proto === null;
}
/**
 * Error thrown when a host callable (a JS `function`) is passed to the
 * *synchronous* call path. See {@link encodeCallArgs} for why this can't work.
 */
class HostCallableSyncError extends Error {
    constructor(message) {
        super(message);
        this.name = 'HostCallableSyncError';
    }
}
exports.HostCallableSyncError = HostCallableSyncError;
function setInboundValue(iv, value, ctx) {
    if (value === null || value === undefined) {
        return; // Leave oneof unset → null
    }
    if (typeof value === 'boolean') {
        iv.boolValue = value;
    }
    else if (typeof value === 'number') {
        if (Number.isInteger(value)) {
            iv.intValue = value;
        }
        else {
            iv.floatValue = value;
        }
    }
    else if (typeof value === 'string') {
        iv.stringValue = value;
    }
    else if (value instanceof Uint8Array) {
        iv.uint8arrayValue = value;
    }
    else if (value instanceof native_1.BamlHandle) {
        iv.handle = { key: value.key, handleType: value.handleType };
    }
    else if (typeof value === 'function') {
        // Host callables cannot work on the synchronous call path —
        // fast-fail before any blocking happens (and before we register a
        // tsfn, which would otherwise be orphaned).
        if (ctx.syncMode) {
            throw new HostCallableSyncError('host callables are only supported on the async call path; use the async API ' +
                '(callFunction) instead of callFunctionSync. The sync path blocks the Node main ' +
                'thread, so the host callback can never run and the call would hang.');
        }
        // JS callable → register a dispatch wrapper in the host-value
        // registry and emit `Handle{key, HOST_VALUE_CALLABLE}`. The Rust
        // side decodes this into `BexExternalValue::HostValue` and binds it
        // to an `Object::HostClosure`; BAML invocations land back in
        // `hostCallableDispatch` below via the ThreadsafeFunction.
        const key = (0, native_1.registerHostCallable)(makeHostCallableDispatch(value));
        // Remember the key so a later encode failure can release it.
        ctx.registered.push(key);
        iv.handle = { key, handleType: BamlHandleType.HOST_VALUE_CALLABLE };
    }
    else if (Array.isArray(value)) {
        const listVal = [];
        for (const item of value) {
            const child = {};
            setInboundValue(child, item, ctx);
            listVal.push(child);
        }
        iv.listValue = { values: listVal };
    }
    else if (isPlainObject(value)) {
        const entries = [];
        for (const [k, v] of Object.entries(value)) {
            const entry = { stringKey: k };
            const childVal = {};
            setInboundValue(childVal, v, ctx);
            entry.value = childVal;
            entries.push(entry);
        }
        iv.mapValue = { entries };
    }
    else {
        throw new TypeError(`Cannot encode value of type ${Object.prototype.toString.call(value)} to protobuf`);
    }
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
function encodeCallArgs(kwargs, syncMode = false) {
    const ctx = { syncMode, registered: [] };
    try {
        const entries = [];
        for (const [key, value] of Object.entries(kwargs)) {
            const entry = { stringKey: key };
            const iv = {};
            setInboundValue(iv, value, ctx);
            entry.value = iv;
            entries.push(entry);
        }
        const msg = CallFunctionArgs.create({ kwargs: entries });
        return Buffer.from(CallFunctionArgs.encode(msg).finish());
    }
    catch (err) {
        // Roll back any host callables registered before the failure so
        // they don't leak in the registry (and pin the libuv loop) for the
        // life of the process — the call never reaches the engine, so the
        // engine would never release them.
        for (const k of ctx.registered) {
            try {
                (0, native_1.releaseHostCallable)(k);
            }
            catch {
                // Best-effort cleanup; never mask the original error.
            }
        }
        throw err;
    }
}
// ─── Outbound (Rust → TS) ───
function decodeValueHolder(holder) {
    if (holder.nullValue != null)
        return null;
    if (holder.stringValue != null)
        return holder.stringValue;
    if (holder.intValue != null)
        return Number(holder.intValue);
    if (holder.floatValue != null)
        return holder.floatValue;
    if (holder.boolValue != null)
        return holder.boolValue;
    if (holder.uint8arrayValue != null)
        return holder.uint8arrayValue;
    if (holder.classValue) {
        const obj = Object.create(null);
        for (const entry of holder.classValue.fields || []) {
            if (entry.key != null && entry.value) {
                obj[entry.key] = decodeValueHolder(entry.value);
            }
        }
        return obj;
    }
    if (holder.enumValue)
        return holder.enumValue.value;
    if (holder.literalValue) {
        if (holder.literalValue.stringLiteral != null)
            return holder.literalValue.stringLiteral.value;
        if (holder.literalValue.intLiteral != null)
            return Number(holder.literalValue.intLiteral.value);
        if (holder.literalValue.boolLiteral != null)
            return holder.literalValue.boolLiteral.value;
    }
    if (holder.listValue) {
        return (holder.listValue.items || []).map(item => decodeValueHolder(item));
    }
    if (holder.mapValue) {
        const obj = Object.create(null);
        for (const entry of holder.mapValue.entries || []) {
            if (entry.key != null && entry.value) {
                obj[entry.key] = decodeValueHolder(entry.value);
            }
        }
        return obj;
    }
    if (holder.unionVariantValue && holder.unionVariantValue.value) {
        return decodeValueHolder(holder.unionVariantValue.value);
    }
    // handle_value: pass the protobufjs Long directly as the key — BamlHandle's
    // constructor accepts { low, high } which is layout-compatible with Long.
    if (holder.handleValue) {
        return new native_1.BamlHandle(holder.handleValue.key, holder.handleValue.handleType ?? 0);
    }
    // FIXME: Unknown/unsupported outbound variants silently collapse to null, making them
    // indistinguishable from a legitimate BAML null result. Legacy engine/ threw via Rust
    // Err/anyhow. bridge_python has the same silent `return None` fallthrough. Leaving as-is
    // for parity with bridge_python; fix both together if this becomes a forward-compat issue.
    return null;
}
function decodeCallResult(data) {
    const msg = BamlOutboundValue.decode(data instanceof Buffer ? data : Buffer.from(data));
    return decodeValueHolder(msg);
}
// ─── Host-callable dispatch (BAML → JS) ───
//
// When BAML invokes a `HostValue` registered via `registerHostCallable`, the
// engine fires the C `HostDispatchFn`, which schedules the per-callable
// `ThreadsafeFunction` (built from the wrapper below) onto the libuv event
// loop with `(callId, argsBytes)`. The wrapper decodes args, invokes the
// user function, encodes the result (or error), and forwards the result
// back to the engine via `completeHostCall`.
//
// Mirrors the Python bridge's `dispatch_in_python` flow
// (sdks/python/rust/bridge_python/src/host_value.rs:152), but the Python
// side calls into `_decode_value_holder` / `_set_inbound_value` directly
// inside the Rust dispatch callback (under the GIL) rather than going
// through a JS-side wrapper. Node's tsfn model makes the wrapper natural.
function makeHostCallableDispatch(userFn) {
    return (callId, argsBytes) => {
        // Every reachable exit from this wrapper must complete `callId`
        // exactly once — if it doesn't, the engine awaits the in-flight call
        // forever (there is no timeout). The outer try/catch is a last-resort
        // net: if anything below throws *after* deciding not to complete (or
        // the normal error path itself throws), we still fire one generic
        // completion. The branches below never both complete and fall
        // through, so we never double-complete.
        try {
            let args;
            try {
                const decoded = decodeCallResult(argsBytes);
                if (!Array.isArray(decoded)) {
                    throw new TypeError(`host-callable args decoded to a non-list value (got ${typeof decoded})`);
                }
                args = decoded;
            }
            catch (err) {
                sendHostCallableError(callId, err);
                return;
            }
            let result;
            try {
                result = userFn(...args);
            }
            catch (err) {
                sendHostCallableError(callId, err);
                return;
            }
            // Async callables: the wrapper resolves the promise on the libuv
            // loop and then forwards the result. The engine has released its
            // heap permit while awaiting `complete_host_call`, so JS-side
            // delay is safe (mirrors the Python `run_until_complete` model).
            if (isPromiseLike(result)) {
                // Adopt the thenable via `Promise.resolve` rather than calling
                // its `.then` directly: a non-compliant thenable could invoke
                // its callbacks more than once, but `Promise.resolve(...)`
                // collapses to a single settlement, so the call completes
                // exactly once. The handlers only call the defended send*
                // helpers (they can't throw synchronously).
                Promise.resolve(result).then((resolved) => sendHostCallableResult(callId, resolved), (err) => sendHostCallableError(callId, err));
            }
            else {
                sendHostCallableResult(callId, result);
            }
        }
        catch (err) {
            // Reached only if a send* helper or the promise plumbing threw
            // *and* did not complete the call. Last-resort completion.
            completeHostCallLastResort(callId, err);
        }
    };
}
function isPromiseLike(value) {
    return (value != null &&
        (typeof value === 'object' || typeof value === 'function') &&
        typeof value.then === 'function');
}
function sendHostCallableResult(callId, value) {
    let bytes;
    try {
        const iv = {};
        // Result-encode path (host → engine): no sync guard (we're already
        // on libuv) and no rollback tracking — the result is handed to the
        // engine, which releases any callables it decodes.
        setInboundValue(iv, value, { syncMode: false, registered: [] });
        const msg = InboundValue.create(iv);
        bytes = Buffer.from(InboundValue.encode(msg).finish());
    }
    catch (err) {
        sendHostCallableError(callId, err);
        return;
    }
    (0, native_1.completeHostCall)(callId, 0, bytes);
}
function sendHostCallableError(callId, err) {
    // This is the normal error path, but it must not be able to leave
    // the call uncompleted. If building/encoding the `HostCallableError`
    // throws (e.g. `describeError`, proto `create`/`encode`, or the native
    // `completeHostCall` itself), fall back to a completion that does the
    // minimum possible work.
    try {
        const { className, message, stack } = describeError(err);
        const msg = HostCallableError.create({
            className,
            message,
            traceback: stack,
            language: 'nodejs',
            category: HostCallableErrorCategory.HOST_CALLABLE_HOST_ERROR,
        });
        const bytes = Buffer.from(HostCallableError.encode(msg).finish());
        (0, native_1.completeHostCall)(callId, 1, bytes);
    }
    catch (innerErr) {
        completeHostCallLastResort(callId, innerErr);
    }
}
/**
 * Absolute last-resort completion. Encodes a fixed, minimal
 * `HostCallableError` with no dependence on the original error object, so the
 * only ways it can fail are a broken proto runtime or a broken native
 * binding — at which point nothing can complete the call. We swallow any
 * throw here to avoid surfacing an unhandled rejection on the libuv loop; the
 * engine's lack of completion would then be the (unavoidable) failure mode.
 */
function completeHostCallLastResort(callId, err) {
    try {
        const msg = HostCallableError.create({
            className: 'InternalError',
            message: `host callable dispatch failed and the error could not be reported: ${safeStringify(err)}`,
            language: 'nodejs',
            category: HostCallableErrorCategory.HOST_CALLABLE_HOST_ERROR,
        });
        const bytes = Buffer.from(HostCallableError.encode(msg).finish());
        (0, native_1.completeHostCall)(callId, 1, bytes);
    }
    catch {
        // Nothing more we can safely do; avoid throwing on the libuv loop.
    }
}
/** `String(err)` that cannot itself throw (e.g. a Proxy with a throwing
 * `toString`). */
function safeStringify(err) {
    try {
        return String(err);
    }
    catch {
        return '<unstringifiable error>';
    }
}
function describeError(err) {
    if (err instanceof Error) {
        return {
            className: err.name || err.constructor.name || 'Error',
            message: err.message || String(err),
            stack: err.stack ?? undefined,
        };
    }
    if (err != null && typeof err === 'object') {
        const ctor = err.constructor;
        const className = ctor?.name && ctor.name !== 'Object' ? ctor.name : 'Error';
        return { className, message: String(err), stack: undefined };
    }
    return { className: 'Error', message: String(err), stack: undefined };
}
//# sourceMappingURL=proto.js.map