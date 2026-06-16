/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
// proto.ts — mirrors bridge_python/python_src/baml_py/proto.py
//
// Encodes TS objects → CallFunctionArgs protobuf bytes (for sending to Rust)
// Decodes the BamlOutboundResult envelope → TS objects (call results), and
// bare BamlOutboundValue bytes → TS objects (host-callable args).
import { baml_core } from './proto/baml_cffi.js';
import { BamlHandle, BamlImage, BamlAudio, BamlVideo, BamlPdf, registerHostCallable, releaseHostCallable, completeHostCall, } from './native.js';
import { BamlStream } from './stream.js';
import { BamlAbortError, BamlCancelledError, BamlError, BamlPanic } from './errors.js';
import { registerHostOpaque, tryRehydrateHostValueByKey, } from './host_value_registry.js';
import { getTypeMap } from './typemap.js';
const CallFunctionArgs = baml_core.cffi.v1.CallFunctionArgs;
const BamlOutboundValue = baml_core.cffi.v1.BamlOutboundValue;
const BamlOutboundResult = baml_core.cffi.v1.BamlOutboundResult;
const InboundValue = baml_core.cffi.v1.InboundValue;
const InboundClassValue = baml_core.cffi.v1.InboundClassValue;
const InboundMapEntry = baml_core.cffi.v1.InboundMapEntry;
const BamlHandleType = baml_core.cffi.v1.BamlHandleType;
const CANCELLED_PANIC_CLASS = 'baml.panics.Cancelled';
// ─── Inbound (TS → Rust) ───
/**
 * Error thrown when a host callable (a JS `function`) is passed to the
 * *synchronous* call path. See {@link encodeCallArgs} for why this can't work.
 */
export class HostCallableSyncError extends Error {
    constructor(message) {
        super(message);
        this.name = 'HostCallableSyncError';
    }
}
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
    else if (typeof value === 'bigint') {
        // Hex / base sixteen on the wire. BigInt.prototype.toString(16)
        // yields e.g. "-2a"; signed values round-trip via num-bigint's
        // LowerHex impl on the Rust side.
        iv.bigintValue = value.toString(16);
    }
    else if (typeof value === 'string') {
        iv.stringValue = value;
    }
    else if (value instanceof Uint8Array) {
        iv.uint8arrayValue = value;
    }
    else if (value instanceof BamlHandle) {
        // A round-tripped host callable arrives as a handle, not a raw
        // function — apply the same sync-path fast-fail so `callFunctionSync`
        // can't hang waiting on a callback that the blocked main thread can
        // never run. HOST_VALUE_CALLABLE is currently the only handle type
        // that dispatches back into the host; the rest are engine-side ADT/
        // heap handles that need no host callback and so are safe on the sync
        // path. Any future dispatch-backed handle type must be guarded here.
        if (ctx.syncMode && value.handleType === BamlHandleType.HOST_VALUE_CALLABLE) {
            throw new HostCallableSyncError('host callables are only supported on the async call path; use the async API ' +
                '(callFunction) instead of callFunctionSync. The sync path blocks the Node main ' +
                'thread, so the host callback can never run and the call would hang.');
        }
        // The Rust inbound decoder drains handle-table entries. Send a fresh
        // cloned key so the JS-owned handle remains valid for later calls.
        iv.handle = { key: value._cloneKeyForWire(), handleType: value.handleType };
    }
    else if (value instanceof BamlStream) {
        // Stream wrapper → its inner TaggedHeapHandle. Mirrors the BamlHandle
        // branch above: the Rust inbound decoder *drains* the handle-table
        // entry, so send a fresh cloned key — otherwise the engine consumes the
        // stream's only key and the next `next()`/`final()` call fails with
        // "Invalid handle key". (`BamlStream._toHandle()` returns the inner
        // handle without cloning, unlike the media wrappers' `_toHandle`.)
        const h = value._toHandle();
        iv.handle = { key: h._cloneKeyForWire(), handleType: h.handleType };
    }
    else if (value instanceof BamlImage
        || value instanceof BamlAudio
        || value instanceof BamlVideo
        || value instanceof BamlPdf) {
        // Stdlib media wrappers → their backing ADT_MEDIA_* handle. `_toHandle`
        // clones the table row so the wrapper stays usable after encode.
        const h = value._toHandle();
        iv.handle = { key: h.key, handleType: h.handleType };
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
        const key = registerHostCallable(makeHostCallableDispatch(value));
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
    else if (value !== null && typeof value === 'object') {
        // Any remaining object — a plain object OR a codegen-emitted class
        // instance (e.g. `new Resume({...})`) — encodes as `map_value` with
        // no FQN tag. The Rust side's `coerce_arg_to_declared_type` reshapes
        // it against the function's declared parameter type (the 10a
        // typemap-free encode simplification). `Object.entries` yields the
        // class's own enumerable fields, set by the constructor's
        // `Object.assign(this, init)`. The specific built-in wrappers
        // (BamlHandle/BamlStream/media) are handled by the instanceof
        // branches above, so they never reach here.
        //
        // Class instances additionally carry their instance-method bindings
        // (`m = defineInstanceFunction(...).bind(this)`) as own enumerable
        // fields. Those are behavior, not state — skip function-valued fields
        // on a class instance so re-encoding a handle-backed value (e.g. a
        // `baml.fs.File` with `read`/`text` bindings) sends only its data
        // (the `_handle`). Plain objects keep every field, so a host callable
        // nested in a plain object still encodes as a callable.
        const proto = Object.getPrototypeOf(value);
        const isClassInstance = proto !== Object.prototype && proto !== null;
        // Handle-backed stdlib types (e.g. `baml.fs.File`, `baml.http.Response`)
        // decode to a class instance that carries the engine's handle in a
        // field (`_handle` / `_body`). The engine resolves these from a
        // FQN-tagged `class_value` (not a bare `map` — which has no FQN — nor a
        // bare `handle`), so re-sending the same handle inside the named class
        // value lets it resolve the same object and preserve cursor/connection
        // state across FFI calls. The FQN comes from the typemap reverse map.
        if (isClassInstance && Object.values(value).some(v => v instanceof BamlHandle)) {
            const fqn = getTypeMap().jsTypeToBamlType(value.constructor);
            if (fqn) {
                const classFields = [];
                for (const [k, v] of Object.entries(value)) {
                    if (typeof v === 'function')
                        continue;
                    const childVal = {};
                    setInboundValue(childVal, v, ctx);
                    classFields.push({ stringKey: k, value: childVal });
                }
                iv.classValue = { name: fqn, fields: classFields };
                return;
            }
        }
        const entries = [];
        for (const [k, v] of Object.entries(value)) {
            if (isClassInstance && typeof v === 'function')
                continue;
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
export function encodeCallArgs(kwargs, options) {
    const callId = options.callId;
    if (callId === 0n) {
        throw new TypeError('callId must be a nonzero uint64');
    }
    const ctx = { syncMode: options.syncMode ?? false, registered: [] };
    try {
        const entries = [];
        for (const [key, value] of Object.entries(kwargs)) {
            const entry = { stringKey: key };
            const iv = {};
            setInboundValue(iv, value, ctx);
            entry.value = iv;
            entries.push(entry);
        }
        const msg = CallFunctionArgs.fromObject({ kwargs: entries, callId: callId.toString() });
        return Buffer.from(CallFunctionArgs.encode(msg).finish());
    }
    catch (err) {
        // Roll back any host callables registered before the failure so
        // they don't leak in the registry (and pin the libuv loop) for the
        // life of the process — the call never reaches the engine, so the
        // engine would never release them.
        for (const k of ctx.registered) {
            try {
                releaseHostCallable(k);
            }
            catch {
                // Best-effort cleanup; never mask the original error.
            }
        }
        throw err;
    }
}
// ─── Outbound (Rust → TS) ───
// Hex / base sixteen on the wire. Shared by `bigint_value` (runtime
// values) and `bigint_literal` (type literals) since both fields use
// the same wire format. BigInt() accepts a "0x"-prefixed hex literal;
// strip a leading minus so we can parse the magnitude. Guard against
// empty or sign-only inputs — `BigInt("0x")` throws `SyntaxError`, so
// we surface a clearer error instead.
// Workspace bigint cap = 2^28 bits ⇒ at most (2^28)/4 hex digits, plus a
// small slack to match the Rust-side `MAX_BIGINT_HEX_LEN` constant in
// `bridge_ctypes/src/value_decode.rs`. Reject longer inputs before calling
// `BigInt()` so a megabyte-scale payload can't drive an unbounded allocation.
const MAX_BIGINT_HEX_LEN = (1 << 28) / 4 + 2;
function parseHexBigint(s) {
    const magnitude = s.startsWith('-') ? s.slice(1) : s;
    if (magnitude.length === 0 || !/^[0-9a-fA-F]+$/.test(magnitude)) {
        throw new Error(`Invalid bigint hex on the wire: ${JSON.stringify(s)}`);
    }
    if (magnitude.length > MAX_BIGINT_HEX_LEN) {
        throw new Error(`bigint hex exceeds the workspace cap (${magnitude.length} chars, limit ${MAX_BIGINT_HEX_LEN})`);
    }
    return s.startsWith('-')
        ? -BigInt(`0x${magnitude}`)
        : BigInt(`0x${magnitude}`);
}
function decodeValueHolder(holder, typeMap) {
    if (holder.nullValue != null)
        return null;
    if (holder.stringValue != null)
        return holder.stringValue;
    if (holder.intValue != null)
        return Number(holder.intValue);
    if (holder.bigintValue != null) {
        return parseHexBigint(holder.bigintValue);
    }
    if (holder.floatValue != null)
        return holder.floatValue;
    if (holder.boolValue != null)
        return holder.boolValue;
    if (holder.uint8arrayValue != null)
        return holder.uint8arrayValue;
    if (holder.classValue) {
        return decodeClass(holder.classValue, typeMap);
    }
    if (holder.enumValue) {
        return decodeEnum(holder.enumValue, typeMap);
    }
    if (holder.literalValue) {
        if (holder.literalValue.stringLiteral != null)
            return holder.literalValue.stringLiteral.value;
        if (holder.literalValue.intLiteral != null)
            return Number(holder.literalValue.intLiteral.value);
        if (holder.literalValue.boolLiteral != null)
            return holder.literalValue.boolLiteral.value;
        // Hex / base sixteen on the wire, matching `bigint_value`. `value`
        // is a required field on `BamlTyLiteralBigint`, so its absence
        // indicates a corrupted / malformed wire message — reject loudly
        // rather than silently coercing a missing field to `0n`.
        if (holder.literalValue.bigintLiteral != null) {
            const hex = holder.literalValue.bigintLiteral.value;
            if (hex == null) {
                throw new Error('wire message: BamlTyLiteralBigint missing required `value`');
            }
            return parseHexBigint(hex);
        }
    }
    if (holder.listValue) {
        return (holder.listValue.items || []).map(item => decodeValueHolder(item, typeMap));
    }
    if (holder.mapValue) {
        const obj = Object.create(null);
        for (const entry of holder.mapValue.entries || []) {
            if (entry.key != null && entry.value) {
                obj[entry.key] = decodeValueHolder(entry.value, typeMap);
            }
        }
        return obj;
    }
    if (holder.unionVariantValue && holder.unionVariantValue.value) {
        return decodeValueHolder(holder.unionVariantValue.value, typeMap);
    }
    // handle_value: pass the protobufjs Long directly as the key — BamlHandle's
    // constructor accepts { low, high } which is layout-compatible with Long.
    // Dispatch on handle_type so media handles decode to their typed wrapper.
    if (holder.handleValue) {
        const ht = holder.handleValue.handleType ?? 0;
        if (ht === BamlHandleType.HANDLE_UNSPECIFIED) {
            // Never a valid decoded handle (mirrors Python's _decode_handle).
            throw new BamlError('decoded handle has HANDLE_UNSPECIFIED handle_type');
        }
        const handle = new BamlHandle(holder.handleValue.key, ht);
        if (ht === BamlHandleType.ADT_MEDIA_IMAGE)
            return BamlImage._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_AUDIO)
            return BamlAudio._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_VIDEO)
            return BamlVideo._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_PDF)
            return BamlPdf._fromHandle(handle);
        if (ht === BamlHandleType.ADT_TAGGED_HEAP_HANDLE) {
            // Dispatch via the typemap: every tagged-heap class self-registers
            // under its engine FQN (codegen emits the entry, e.g.
            // `baml.llm.Stream → BamlStream`), so any class is reachable without
            // special-casing here. Mirrors bridge_python's `_decode_handle`
            // ADT_TAGGED_HEAP_HANDLE arm (sdks/python/.../proto.py).
            const fqn = holder.handleValue.name?.name ?? '';
            const Cls = typeMap.getClass(fqn);
            return Cls._fromHandle(handle);
        }
        // ADT_MEDIA_GENERIC has no typed wrapper — stays a bare BamlHandle.
        return handle;
    }
    // Inline media / prompt AST are not expected on the Node FFI path — they
    // travel via `handle_value`. Reject loudly rather than silently collapsing
    // to null (mirrors bridge_python's proto.py, which raises here).
    if (holder.mediaValue || holder.promptAstValue) {
        const which = holder.mediaValue ? 'media_value' : 'prompt_ast_value';
        throw new BamlError(`BEX emitted ${which} on the FFI path — media/prompt AST are expected ` +
            `via handle_value, not inline`);
    }
    // Any remaining unset oneof is a legitimate null: an all-default holder is a
    // null BAML result.
    return null;
}
/**
 * Decode a `class_value` to a typed instance via the typemap. When the FQN is
 * in the typemap (the generated-SDK path), construct `new Cls(fieldDict)`
 * (codegen emits `constructor(init) { Object.assign(this, init); }`). The five
 * stdlib media wrappers unwrap their `_data` envelope to the wrapper itself.
 * When the FQN is absent (the bare bridge has no typemap, or an unmapped
 * class), fall back to a plain object — preserving the pre-typemap behavior.
 */
function decodeClass(classValue, typeMap) {
    const fieldDict = {};
    for (const entry of classValue.fields || []) {
        if (entry.key != null && entry.value) {
            fieldDict[entry.key] = decodeValueHolder(entry.value, typeMap);
        }
    }
    const fqn = classValue.name?.name ?? '';
    if (fqn) {
        let Cls;
        try {
            Cls = typeMap.getClass(fqn);
        }
        catch {
            Cls = undefined; // unmapped FQN — fall back below
        }
        if (Cls !== undefined) {
            // Stdlib media wrappers: the decoded `_data` is already the typed
            // wrapper (its inner handle_value decoded via the media branch);
            // unwrap the envelope per the spec's Instance row.
            if ((Cls === BamlImage || Cls === BamlAudio || Cls === BamlVideo || Cls === BamlPdf)
                && '_data' in fieldDict) {
                return fieldDict._data;
            }
            const Ctor = Cls;
            return new Ctor(fieldDict);
        }
    }
    // Fallback: plain object (null-prototype, matching the prior behavior).
    const obj = Object.create(null);
    for (const [k, v] of Object.entries(fieldDict))
        obj[k] = v;
    return obj;
}
/**
 * Decode an `enum_value` to a typed enum member via the typemap. Falls back to
 * the raw variant string when the FQN is unmapped (bare bridge / unmapped enum).
 */
function decodeEnum(enumValue, typeMap) {
    const fqn = enumValue.name?.name ?? '';
    const variant = enumValue.value;
    if (fqn && variant != null) {
        let En;
        try {
            En = typeMap.getEnum(fqn);
        }
        catch {
            En = undefined;
        }
        if (En !== undefined && variant in En) {
            return En[variant];
        }
    }
    return variant;
}
/**
 * Decode a bare `BamlOutboundValue` to a JS value. Used for the host-callable
 * args path, where the engine sends a list-shaped `BamlOutboundValue` rather
 * than the call-result `BamlOutboundResult` envelope.
 */
export function decodeOutboundValue(data) {
    const msg = BamlOutboundValue.decode(data instanceof Buffer ? data : Buffer.from(data));
    return decodeValueHolder(msg, getTypeMap());
}
/**
 * Decode the thrown value off the wire holder. Returns the fully decoded BAML
 * `value` (a generated class instance when the FQN is mapped, else a plain
 * object / primitive), the class FQN (`className`), and a readable `message`
 * lifted from the value's `message` field when present. Mirrors
 * bridge_python's `decode_value` + `_outbound_class_fqn` so the surfaced
 * `BamlError`/`BamlPanic` carries the decoded value, not just a string.
 *
 * Decoding is defensive: a malformed/unsupported thrown payload must not mask
 * the original error/panic, so a decode failure degrades to an undefined value
 * (the formatted message and className are still surfaced).
 */
function decodeThrown(holder) {
    const className = holder?.classValue?.name?.name ?? undefined;
    let value;
    try {
        value = holder ? decodeValueHolder(holder, getTypeMap()) : undefined;
    }
    catch {
        value = undefined;
    }
    let message = '';
    if (value != null && typeof value === 'object' && 'message' in value) {
        const m = value.message;
        if (typeof m === 'string')
            message = m;
    }
    return { value, className, message };
}
function formatThrownMessage(kind, className, message, trace) {
    const label = className || `baml.${kind}`;
    let text = `baml ${kind}: ${label}`;
    if (message)
        text += `: ${message}`;
    if (trace.length)
        text += '\n' + trace.map(l => '    ' + l).join('\n');
    return text;
}
function cancellationAbortError(message) {
    return new BamlAbortError(message, {
        reason: new BamlCancelledError(message),
    });
}
/**
 * Decode a `BamlOutboundResult` envelope (the engine's call-result wire shape
 * after 31c/31e). The `ok` arm returns the decoded value; the `error`/`panic`
 * arms **throw** a `BamlError`/`BamlPanic` carrying the fully decoded thrown
 * value (`.value`), the BAML trace (`.bamlTrace`), and the class FQN
 * (`.className`), with a readable formatted `.message`. An `is_exit_panic`
 * (clean `baml.sys.exit`) terminates the process via `process.exit(code)`
 * rather than throwing.
 */
export function decodeCallResult(data) {
    const buf = data instanceof Buffer ? data : Buffer.from(data);
    const result = BamlOutboundResult.decode(buf);
    switch (result.result) {
        case 'error': {
            const { value, className, message } = decodeThrown(result.error?.value);
            const trace = result.error?.trace ?? [];
            // Same-host rehydration: a `baml.errors.HostCallable` carrying a
            // `_handle` that still resolves in this process's host-value
            // registry re-throws the *original* JS error object the bridge
            // registered on the inbound throw — preserving `raised === caught`
            // identity. Foreign runtimes (a different Node process, the
            // Python bridge) and released keys fall through to the
            // metadata-bearing `BamlError(HostCallable)` wrapper below.
            if (className === 'baml.errors.HostCallable' && value !== null && typeof value === 'object') {
                const handle = value._handle;
                const original = tryRehydrateHostValueByKey(handle);
                if (original !== undefined) {
                    throw original;
                }
            }
            const formatted = formatThrownMessage('error', className ?? '', message, trace);
            if (className === CANCELLED_PANIC_CLASS) {
                throw cancellationAbortError(formatted);
            }
            throw new BamlError(formatted, { value, bamlTrace: trace, className });
        }
        case 'panic': {
            const panic = result.panic;
            if (panic?.isExitPanic) {
                // Clean process-exit panic: exit after flushing telemetry (the
                // registered `process.once('exit', flushEvents)` hook fires
                // synchronously inside process.exit), rather than throwing.
                const code = Number(panic.exitCode ?? 0);
                process.exit(code);
            }
            const { value, className, message } = decodeThrown(panic?.value);
            const trace = panic?.trace ?? [];
            const formatted = formatThrownMessage('panic', className ?? '', message, trace);
            if (className === CANCELLED_PANIC_CLASS) {
                throw cancellationAbortError(formatted);
            }
            throw new BamlPanic(formatted, { value, bamlTrace: trace, className });
        }
        case 'ok':
        default:
            // `ok` (or an absent oneof — an all-default envelope is a null `ok`).
            return result.ok ? decodeValueHolder(result.ok, getTypeMap()) : null;
    }
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
                // Host-callable args arrive as a bare list-shaped
                // BamlOutboundValue, not the call-result envelope.
                const decoded = decodeOutboundValue(argsBytes);
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
    // Result-encode path (host → engine): no sync guard (we're already on
    // libuv). We do track registrations, though — a callable nested in the
    // result is registered before encoding finishes, and if encoding then
    // throws, the bytes never reach the engine, so it never decodes (and
    // never releases) the callable. Roll those back on failure, mirroring the
    // argument-path rollback in `encodeCallArgs`.
    const ctx = { syncMode: false, registered: [] };
    try {
        const iv = {};
        setInboundValue(iv, value, ctx);
        const msg = InboundValue.create(iv);
        bytes = Buffer.from(InboundValue.encode(msg).finish());
    }
    catch (err) {
        for (const k of ctx.registered) {
            try {
                releaseHostCallable(k);
            }
            catch {
                // Best-effort cleanup; never mask the original error.
            }
        }
        sendHostCallableError(callId, err);
        return;
    }
    completeHostCall(callId, 0, bytes);
}
/**
 * Build an `InboundValue` carrying a `baml.errors.HostCallable` Instance with
 * the four metadata fields. The engine's `materialize_host_throw` runs the
 * declared-throws contract check against this value and either re-injects it
 * as a catchable BAML throw or escalates to a `HostContractViolation` panic.
 */
function buildHostCallableInbound(className, message, traceback, handleKey) {
    const stringField = (key, value) => InboundMapEntry.create({
        stringKey: key,
        value: InboundValue.create({ stringValue: value }),
    });
    const handleField = InboundMapEntry.create({
        stringKey: '_handle',
        value: InboundValue.create({
            handle: {
                key: handleKey,
                handleType: BamlHandleType.HOST_VALUE_OPAQUE,
            },
        }),
    });
    const fields = [
        stringField('message', message),
        stringField('class_name', className),
        stringField('language', 'nodejs'),
    ];
    if (traceback != null) {
        fields.push(stringField('traceback', traceback));
    }
    fields.push(handleField);
    return InboundValue.create({
        classValue: InboundClassValue.create({
            name: 'baml.errors.HostCallable',
            fields,
        }),
    });
}
// Sentinel `_handle` key used by paths that have *no* JS error object to
// register (the `completeHostCallLastResort` fallback below). Real host
// throws register the JS error via `registerHostOpaque` and emit its
// minted key; engine-internal synthetic faults use this sentinel. The
// engine's structural check accepts either; same-host decoders that look
// up `{low:0,high:0}` find nothing and fall through to the
// metadata-bearing `BamlError(HostCallable)` wrapper. Mirrors the
// reserved sentinel in `bridge_python::host_value::next_key` (which
// skips `0`), so a real registered key can never collide.
//
// Cast through `as unknown as HandleKey` because `HandleKey` is a native
// napi class with private fields; protobufjs only reads the public
// `{low, high}` shape and the wire serializes them identically.
const UNRESOLVED_HOST_ERROR_KEY = { low: 0, high: 0 };
function sendHostCallableError(callId, err) {
    // Normal error path: never leaves the call uncompleted. If building or
    // encoding the Instance throws (e.g. `describeError`, proto `create` /
    // `encode`, or the native `completeHostCall` itself), fall back to the
    // last-resort completion below.
    try {
        // BAML-error-unwrap path: `BamlError(value=<codegenned BAML class>)`.
        // The user wrapped a real BAML class (or primitive) in a BamlError;
        // emit it as that real BAML class on the wire so the BAML caller's
        // typed `catch (e: MyError)` matches structurally and reads typed
        // fields. Mirrors `bridge_python`'s `try_encode_baml_error_throw`.
        //
        // `value == null` (a bare `new BamlError("msg")` with no wrapped
        // value, or `new BamlError("", { value: null })`) falls through to
        // the opaque-handle fallback below: an unset / null inner oneof
        // encodes as BAML `null`, which fails the engine's contract check
        // for any concrete `E` (and produces a bizarre `null` throw for
        // `E=unknown`). The opaque path always produces a well-formed
        // `HostCallable` instance the engine can route.
        //
        // `BamlPanic extends BamlError`, so a `BamlPanic(value=...)` also
        // hits this branch. The engine routes by BAML class namespace
        // (`baml.panics.*` → panic, `baml.errors.*` / user classes →
        // error), so a properly-constructed `BamlPanic(value=<panic class>)`
        // surfaces as a panic; a `BamlPanic(value=<error class>)` surfaces
        // as an error. Treating both via the same wire path is consistent
        // with how engine-internal throws are routed.
        if (err instanceof BamlError && err.value != null) {
            const ctx = { syncMode: false, registered: [] };
            try {
                const iv = InboundValue.create({});
                setInboundValue(iv, err.value, ctx);
                const bytes = Buffer.from(InboundValue.encode(iv).finish());
                completeHostCall(callId, 1, bytes);
                return;
            }
            catch {
                // Encoding the inner value failed — roll back any callable
                // registrations its nested fields triggered and fall through
                // to the opaque-handle path below. The fall-through is
                // intentional: the throw must still reach the engine even if
                // the typed-value encode broke, so we never leave the call
                // hanging. Each release is wrapped in its own try/catch so
                // a single bad entry doesn't abort the rest of the rollback
                // and leak the remaining registrations (mirrors the other
                // rollback sites in `setInboundValue` and
                // `sendHostCallableResult`).
                for (const key of ctx.registered) {
                    try {
                        releaseHostCallable(key);
                    }
                    catch {
                        // Best-effort cleanup; never mask the original error.
                    }
                }
            }
        }
        // Opaque-handle path: a `baml.errors.HostCallable` Instance carrying
        // a handle to the originating JS error so the BAML→host decoder on
        // the same Node process can rehydrate the *same* exception object
        // on round-trip (`raised === caught` identity). Foreign runtimes /
        // released keys fall back to metadata.
        //
        // Identity-loss edge case (acceptable): `describeError` can itself
        // throw when given a hostile input (e.g. a Proxy whose `constructor`,
        // `name`, or `message` getters throw — see the
        // `host-callable always completes on abnormal paths` jest suite).
        // In that case `registerHostOpaque` is never reached, control jumps
        // to the outer `catch (innerErr)` → `completeHostCallLastResort`,
        // and the user sees a metadata-only `HostCallable` instead of the
        // original Proxy. Identity loss here is the right trade — the
        // alternative is hanging the call.
        //
        // Registration-leak edge case (rare, bounded): if encoding succeeds
        // through `registerHostOpaque` but `buildHostCallableInbound` or
        // `InboundValue.encode` fails after, the TS map entry stays alive
        // with no corresponding engine-side `HostValueArc` to release it,
        // so it lives until process exit. Both downstream calls are deeply
        // mechanical (build a proto message, serialize fixed-shape fields)
        // and don't depend on `err`'s shape, so this is effectively
        // unreachable outside protobufjs / native-binding corruption.
        const { className, message, stack } = describeError(err);
        const handleKey = registerHostOpaque(err);
        const inbound = buildHostCallableInbound(className, message, stack, handleKey);
        const bytes = Buffer.from(InboundValue.encode(inbound).finish());
        completeHostCall(callId, 1, bytes);
    }
    catch (innerErr) {
        completeHostCallLastResort(callId, innerErr);
    }
}
/**
 * Absolute last-resort completion. Encodes a fixed, minimal
 * `baml.errors.HostCallable` Instance with no dependence on the original
 * error object, so the only ways it can fail are a broken proto runtime or a
 * broken native binding — at which point nothing can complete the call. We
 * swallow any throw here to avoid surfacing an unhandled rejection on the
 * libuv loop; the engine's lack of completion would then be the
 * (unavoidable) failure mode.
 */
function completeHostCallLastResort(callId, err) {
    try {
        const inbound = buildHostCallableInbound('InternalError', `host callable dispatch failed and the error could not be reported: ${safeStringify(err)}`, undefined, UNRESOLVED_HOST_ERROR_KEY);
        const bytes = Buffer.from(InboundValue.encode(inbound).finish());
        completeHostCall(callId, 1, bytes);
    }
    catch (innerErr) {
        // Nothing more we can safely do — a throw here would surface as an
        // unhandled rejection on the libuv loop. The engine's lack of
        // completion will then be the (unavoidable) failure mode; log so the
        // failure is at least attributable.
        console.error('BAML internal: last-resort host-call completion failed; call will hang.', { callId, originalError: safeStringify(err), lastResortError: innerErr });
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