// proto.ts — mirrors bridge_python/python_src/baml_py/proto.py
//
// Encodes TS objects → CallFunctionArgs protobuf bytes (for sending to Rust)
// Decodes the BamlOutboundResult envelope → TS objects (call results), and
// bare BamlOutboundValue bytes → TS objects (host-callable args).

import { baml_core } from './proto/baml_cffi.js';
import {
    BamlHandle,
    HandleKey,
    putHandleIntoTable,
    BamlImage,
    BamlAudio,
    BamlVideo,
    BamlPdf,
    registerHostCallable,
    releaseHostCallable,
    completeHostCall,
} from './native.js';
import { BamlStream } from './stream.js';
import { BamlError, BamlPanic } from './errors.js';
import { BamlTypeMap, getTypeMap } from './typemap.js';

const CallFunctionArgs = baml_core.cffi.v1.CallFunctionArgs;
const BamlOutboundValue = baml_core.cffi.v1.BamlOutboundValue;
const BamlOutboundResult = baml_core.cffi.v1.BamlOutboundResult;
const InboundValue = baml_core.cffi.v1.InboundValue;
const HostCallableError = baml_core.cffi.v1.HostCallableError;
const HostCallableErrorCategory = baml_core.cffi.v1.HostCallableErrorCategory;
const BamlHandleType = baml_core.cffi.v1.BamlHandleType;

// ─── Inbound (TS → Rust) ───

/**
 * Error thrown when a host callable (a JS `function`) is passed to the
 * *synchronous* call path. See {@link encodeCallArgs} for why this can't work.
 */
export class HostCallableSyncError extends Error {
    constructor(message: string) {
        super(message);
        this.name = 'HostCallableSyncError';
    }
}

/**
 * Per-encode state threaded through {@link setInboundValue}.
 *
 * - `syncMode`: when true, encountering a host callable fast-fails. The
 *   sync call path blocks the Node main thread on a tokio `block_on`, which
 *   starves libuv so the `ThreadsafeFunction` dispatch can never run — the
 *   user callback would never fire, `completeHostCall` would never be
 *   called, and the engine would await the in-flight `call_id` forever. We
 *   reject before blocking instead of hanging.
 * - `registered`: keys minted while encoding this call, so an encode error
 *   on a later kwarg can roll back the registrations of earlier ones.
 */
interface EncodeCtx {
    syncMode: boolean;
    registered: HandleKey[];
}

function setInboundValue(iv: baml_core.cffi.v1.IInboundValue, value: unknown, ctx: EncodeCtx): void {
    if (value === null || value === undefined) {
        return; // Leave oneof unset → null
    }
    if (typeof value === 'boolean') {
        iv.boolValue = value;
    } else if (typeof value === 'number') {
        if (Number.isInteger(value)) {
            iv.intValue = value;
        } else {
            iv.floatValue = value;
        }
    } else if (typeof value === 'bigint') {
        // Hex / base sixteen on the wire (see Phase 10 of the bigint plan).
        // BigInt.prototype.toString(16) yields e.g. "-2a"; signed values
        // round-trip via num-bigint's LowerHex impl on the Rust side.
        iv.bigintValue = value.toString(16);
    } else if (typeof value === 'string') {
        iv.stringValue = value;
    } else if (value instanceof Uint8Array) {
        iv.uint8arrayValue = value;
    } else if (value instanceof BamlHandle) {
        // A round-tripped host callable arrives as a handle, not a raw
        // function — apply the same sync-path fast-fail so `callFunctionSync`
        // can't hang waiting on a callback that the blocked main thread can
        // never run. HOST_VALUE_CALLABLE is currently the only handle type
        // that dispatches back into the host; the rest are engine-side ADT/
        // heap handles that need no host callback and so are safe on the sync
        // path. Any future dispatch-backed handle type must be guarded here.
        if (ctx.syncMode && value.handleType === BamlHandleType.HOST_VALUE_CALLABLE) {
            throw new HostCallableSyncError(
                'host callables are only supported on the async call path; use the async API ' +
                '(callFunction) instead of callFunctionSync. The sync path blocks the Node main ' +
                'thread, so the host callback can never run and the call would hang.'
            );
        }
        // The Rust inbound decoder drains handle-table entries. Send a fresh
        // cloned key so the JS-owned handle remains valid for later calls.
        iv.handle = { key: putHandleIntoTable(value), handleType: value.handleType };
    } else if (value instanceof BamlStream) {
        // Stream wrapper → its inner TaggedHeapHandle. Mirrors the BamlHandle
        // branch above; the engine re-binds it to the heap value on decode.
        const h = value._toHandle();
        iv.handle = { key: h.key, handleType: h.handleType };
    } else if (
        value instanceof BamlImage
        || value instanceof BamlAudio
        || value instanceof BamlVideo
        || value instanceof BamlPdf
    ) {
        // Stdlib media wrappers → their backing ADT_MEDIA_* handle. `_toHandle`
        // clones the table row so the wrapper stays usable after encode.
        const h = value._toHandle();
        iv.handle = { key: h.key, handleType: h.handleType };
    } else if (typeof value === 'function') {
        // Host callables cannot work on the synchronous call path —
        // fast-fail before any blocking happens (and before we register a
        // tsfn, which would otherwise be orphaned).
        if (ctx.syncMode) {
            throw new HostCallableSyncError(
                'host callables are only supported on the async call path; use the async API ' +
                '(callFunction) instead of callFunctionSync. The sync path blocks the Node main ' +
                'thread, so the host callback can never run and the call would hang.'
            );
        }
        // JS callable → register a dispatch wrapper in the host-value
        // registry and emit `Handle{key, HOST_VALUE_CALLABLE}`. The Rust
        // side decodes this into `BexExternalValue::HostValue` and binds it
        // to an `Object::HostClosure`; BAML invocations land back in
        // `hostCallableDispatch` below via the ThreadsafeFunction.
        const key = registerHostCallable(makeHostCallableDispatch(value as (...args: unknown[]) => unknown));
        // Remember the key so a later encode failure can release it.
        ctx.registered.push(key);
        iv.handle = { key, handleType: BamlHandleType.HOST_VALUE_CALLABLE };
    } else if (Array.isArray(value)) {
        const listVal: baml_core.cffi.v1.IInboundValue[] = [];
        for (const item of value) {
            const child: baml_core.cffi.v1.IInboundValue = {};
            setInboundValue(child, item, ctx);
            listVal.push(child);
        }
        iv.listValue = { values: listVal };
    } else if (value !== null && typeof value === 'object') {
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
            const fqn = getTypeMap().jsTypeToBamlType((value as object).constructor);
            if (fqn) {
                const classFields: baml_core.cffi.v1.IInboundMapEntry[] = [];
                for (const [k, v] of Object.entries(value)) {
                    if (typeof v === 'function') continue;
                    const childVal: baml_core.cffi.v1.IInboundValue = {};
                    setInboundValue(childVal, v, ctx);
                    classFields.push({ stringKey: k, value: childVal });
                }
                iv.classValue = { name: fqn, fields: classFields };
                return;
            }
        }
        const entries: baml_core.cffi.v1.IInboundMapEntry[] = [];
        for (const [k, v] of Object.entries(value)) {
            if (isClassInstance && typeof v === 'function') continue;
            const entry: baml_core.cffi.v1.IInboundMapEntry = { stringKey: k };
            const childVal: baml_core.cffi.v1.IInboundValue = {};
            setInboundValue(childVal, v, ctx);
            entry.value = childVal;
            entries.push(entry);
        }
        iv.mapValue = { entries };
    } else {
        throw new TypeError(
            `Cannot encode value of type ${Object.prototype.toString.call(value)} to protobuf`
        );
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
export function encodeCallArgs(kwargs: Record<string, unknown>, syncMode = false): Buffer {
    const ctx: EncodeCtx = { syncMode, registered: [] };
    try {
        const entries: baml_core.cffi.v1.IInboundMapEntry[] = [];
        for (const [key, value] of Object.entries(kwargs)) {
            const entry: baml_core.cffi.v1.IInboundMapEntry = { stringKey: key };
            const iv: baml_core.cffi.v1.IInboundValue = {};
            setInboundValue(iv, value, ctx);
            entry.value = iv;
            entries.push(entry);
        }
        const msg = CallFunctionArgs.create({ kwargs: entries });
        return Buffer.from(CallFunctionArgs.encode(msg).finish());
    } catch (err) {
        // Roll back any host callables registered before the failure so
        // they don't leak in the registry (and pin the libuv loop) for the
        // life of the process — the call never reaches the engine, so the
        // engine would never release them.
        for (const k of ctx.registered) {
            try {
                releaseHostCallable(k);
            } catch {
                // Best-effort cleanup; never mask the original error.
            }
        }
        throw err;
    }
}

// ─── Outbound (Rust → TS) ───

// Hex / base sixteen on the wire (see Phase 10 of the bigint plan). Shared
// by `bigint_value` (runtime values) and `bigint_literal` (type literals)
// since both fields use the same wire format. BigInt() accepts a "0x"-prefixed
// hex literal; strip a leading minus so we can parse the magnitude. Guard
// against empty or sign-only inputs — `BigInt("0x")` throws `SyntaxError`,
// so we surface a clearer error instead.
// Workspace bigint cap = 2^28 bits ⇒ at most (2^28)/4 hex digits, plus a
// small slack to match the Rust-side `MAX_BIGINT_HEX_LEN` constant in
// `bridge_ctypes/src/value_decode.rs`. Reject longer inputs before calling
// `BigInt()` so a megabyte-scale payload can't drive an unbounded allocation.
const MAX_BIGINT_HEX_LEN = (1 << 28) / 4 + 2;
function parseHexBigint(s: string): bigint {
    const magnitude = s.startsWith('-') ? s.slice(1) : s;
    if (magnitude.length === 0 || !/^[0-9a-fA-F]+$/.test(magnitude)) {
        throw new Error(
            `Invalid bigint hex on the wire: ${JSON.stringify(s)}`,
        );
    }
    if (magnitude.length > MAX_BIGINT_HEX_LEN) {
        throw new Error(
            `bigint hex exceeds the workspace cap (${magnitude.length} chars, limit ${MAX_BIGINT_HEX_LEN})`,
        );
    }
    return s.startsWith('-')
        ? -BigInt(`0x${magnitude}`)
        : BigInt(`0x${magnitude}`);
}

function decodeValueHolder(
    holder: baml_core.cffi.v1.IBamlOutboundValue,
    typeMap: BamlTypeMap,
): unknown {
    if (holder.nullValue != null) return null;
    if (holder.stringValue != null) return holder.stringValue;
    if (holder.intValue != null) return Number(holder.intValue);
    if (holder.bigintValue != null) {
        return parseHexBigint(holder.bigintValue as string);
    }
    if (holder.floatValue != null) return holder.floatValue;
    if (holder.boolValue != null) return holder.boolValue;
    if (holder.uint8arrayValue != null) return holder.uint8arrayValue;
    if (holder.classValue) {
        return decodeClass(holder.classValue, typeMap);
    }
    if (holder.enumValue) {
        return decodeEnum(holder.enumValue, typeMap);
    }
    if (holder.literalValue) {
        if (holder.literalValue.stringLiteral != null) return holder.literalValue.stringLiteral.value;
        if (holder.literalValue.intLiteral != null) return Number(holder.literalValue.intLiteral.value);
        if (holder.literalValue.boolLiteral != null) return holder.literalValue.boolLiteral.value;
        // Hex / base sixteen on the wire, matching `bigint_value`. `value`
        // is a required field on `BamlTyLiteralBigint`, so its absence
        // indicates a corrupted / malformed wire message — reject loudly
        // rather than silently coercing a missing field to `0n`.
        if (holder.literalValue.bigintLiteral != null) {
            const hex = holder.literalValue.bigintLiteral.value;
            if (hex == null) {
                throw new Error(
                    'wire message: BamlTyLiteralBigint missing required `value`',
                );
            }
            return parseHexBigint(hex);
        }
    }
    if (holder.listValue) {
        return (holder.listValue.items || []).map(item => decodeValueHolder(item, typeMap));
    }
    if (holder.mapValue) {
        const obj: Record<string, unknown> = Object.create(null);
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
        if (ht === BamlHandleType.ADT_MEDIA_IMAGE) return BamlImage._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_AUDIO) return BamlAudio._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_VIDEO) return BamlVideo._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_PDF) return BamlPdf._fromHandle(handle);
        // ADT_MEDIA_GENERIC has no typed wrapper — stays a bare BamlHandle.
        // TODO: ADT_TAGGED_HEAP_HANDLE / RustData re-encode (handle-backed
        // stdlib types like baml.fs.File) needs cross-call handle-lifecycle
        // work; for now non-media handles decode to a bare BamlHandle.
        return handle;
    }
    // Inline media / prompt AST are not expected on the Node FFI path — they
    // travel via `handle_value`. Reject loudly rather than silently collapsing
    // to null (mirrors bridge_python's proto.py, which raises here).
    if (holder.mediaValue || holder.promptAstValue) {
        const which = holder.mediaValue ? 'media_value' : 'prompt_ast_value';
        throw new BamlError(
            `BEX emitted ${which} on the FFI path — media/prompt AST are expected ` +
            `via handle_value, not inline`,
        );
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
function decodeClass(
    classValue: baml_core.cffi.v1.IBamlValueClass,
    typeMap: BamlTypeMap,
): unknown {
    const fieldDict: Record<string, unknown> = {};
    for (const entry of classValue.fields || []) {
        if (entry.key != null && entry.value) {
            fieldDict[entry.key] = decodeValueHolder(entry.value, typeMap);
        }
    }
    const fqn = classValue.name?.name ?? '';
    if (fqn) {
        let Cls: unknown;
        try {
            Cls = typeMap.getClass(fqn);
        } catch {
            Cls = undefined; // unmapped FQN — fall back below
        }
        if (Cls !== undefined) {
            // Stdlib media wrappers: the decoded `_data` is already the typed
            // wrapper (its inner handle_value decoded via the media branch);
            // unwrap the envelope per the spec's Instance row.
            if (
                (Cls === BamlImage || Cls === BamlAudio || Cls === BamlVideo || Cls === BamlPdf)
                && '_data' in fieldDict
            ) {
                return fieldDict._data;
            }
            const Ctor = Cls as new (init: Record<string, unknown>) => unknown;
            return new Ctor(fieldDict);
        }
    }
    // Fallback: plain object (null-prototype, matching the prior behavior).
    const obj: Record<string, unknown> = Object.create(null);
    for (const [k, v] of Object.entries(fieldDict)) obj[k] = v;
    return obj;
}

/**
 * Decode an `enum_value` to a typed enum member via the typemap. Falls back to
 * the raw variant string when the FQN is unmapped (bare bridge / unmapped enum).
 */
function decodeEnum(
    enumValue: baml_core.cffi.v1.IBamlValueEnum,
    typeMap: BamlTypeMap,
): unknown {
    const fqn = enumValue.name?.name ?? '';
    const variant = enumValue.value;
    if (fqn && variant != null) {
        let En: unknown;
        try {
            En = typeMap.getEnum(fqn);
        } catch {
            En = undefined;
        }
        if (En !== undefined && variant in (En as Record<string, unknown>)) {
            return (En as Record<string, unknown>)[variant];
        }
    }
    return variant;
}

/**
 * Decode a bare `BamlOutboundValue` to a JS value. Used for the host-callable
 * args path, where the engine sends a list-shaped `BamlOutboundValue` rather
 * than the call-result `BamlOutboundResult` envelope.
 */
export function decodeOutboundValue(data: Buffer | Uint8Array): unknown {
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
function decodeThrown(
    holder: baml_core.cffi.v1.IBamlOutboundValue | null | undefined
): { value: unknown; className: string | undefined; message: string } {
    const className = holder?.classValue?.name?.name ?? undefined;
    let value: unknown;
    try {
        value = holder ? decodeValueHolder(holder, getTypeMap()) : undefined;
    } catch {
        value = undefined;
    }
    let message = '';
    if (value != null && typeof value === 'object' && 'message' in (value as object)) {
        const m = (value as Record<string, unknown>).message;
        if (typeof m === 'string') message = m;
    }
    return { value, className, message };
}

function formatThrownMessage(kind: string, className: string, message: string, trace: string[]): string {
    const label = className || `baml.${kind}`;
    let text = `baml ${kind}: ${label}`;
    if (message) text += `: ${message}`;
    if (trace.length) text += '\n' + trace.map(l => '    ' + l).join('\n');
    return text;
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
export function decodeCallResult(data: Buffer | Uint8Array): unknown {
    const buf = data instanceof Buffer ? data : Buffer.from(data);
    const result = BamlOutboundResult.decode(buf);
    switch (result.result) {
        case 'error': {
            const { value, className, message } = decodeThrown(result.error?.value);
            const trace = result.error?.trace ?? [];
            throw new BamlError(
                formatThrownMessage('error', className ?? '', message, trace),
                { value, bamlTrace: trace, className },
            );
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
            throw new BamlPanic(
                formatThrownMessage('panic', className ?? '', message, trace),
                { value, bamlTrace: trace, className },
            );
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

function makeHostCallableDispatch(userFn: (...args: unknown[]) => unknown) {
    return (callId: number, argsBytes: Buffer): void => {
        // Every reachable exit from this wrapper must complete `callId`
        // exactly once — if it doesn't, the engine awaits the in-flight call
        // forever (there is no timeout). The outer try/catch is a last-resort
        // net: if anything below throws *after* deciding not to complete (or
        // the normal error path itself throws), we still fire one generic
        // completion. The branches below never both complete and fall
        // through, so we never double-complete.
        try {
            let args: unknown[];
            try {
                // Host-callable args arrive as a bare list-shaped
                // BamlOutboundValue, not the call-result envelope.
                const decoded = decodeOutboundValue(argsBytes);
                if (!Array.isArray(decoded)) {
                    throw new TypeError(
                        `host-callable args decoded to a non-list value (got ${typeof decoded})`
                    );
                }
                args = decoded;
            } catch (err) {
                sendHostCallableError(callId, err);
                return;
            }
            let result: unknown;
            try {
                result = userFn(...args);
            } catch (err) {
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
                Promise.resolve(result).then(
                    (resolved) => sendHostCallableResult(callId, resolved),
                    (err) => sendHostCallableError(callId, err)
                );
            } else {
                sendHostCallableResult(callId, result);
            }
        } catch (err) {
            // Reached only if a send* helper or the promise plumbing threw
            // *and* did not complete the call. Last-resort completion.
            completeHostCallLastResort(callId, err);
        }
    };
}

function isPromiseLike(value: unknown): value is PromiseLike<unknown> {
    return (
        value != null &&
        (typeof value === 'object' || typeof value === 'function') &&
        typeof (value as { then?: unknown }).then === 'function'
    );
}

function sendHostCallableResult(callId: number, value: unknown): void {
    let bytes: Buffer;
    // Result-encode path (host → engine): no sync guard (we're already on
    // libuv). We do track registrations, though — a callable nested in the
    // result is registered before encoding finishes, and if encoding then
    // throws, the bytes never reach the engine, so it never decodes (and
    // never releases) the callable. Roll those back on failure, mirroring the
    // argument-path rollback in `encodeCallArgs`.
    const ctx: EncodeCtx = { syncMode: false, registered: [] };
    try {
        const iv: baml_core.cffi.v1.IInboundValue = {};
        setInboundValue(iv, value, ctx);
        const msg = InboundValue.create(iv);
        bytes = Buffer.from(InboundValue.encode(msg).finish());
    } catch (err) {
        for (const k of ctx.registered) {
            try {
                releaseHostCallable(k);
            } catch {
                // Best-effort cleanup; never mask the original error.
            }
        }
        sendHostCallableError(callId, err);
        return;
    }
    completeHostCall(callId, 0, bytes);
}

function sendHostCallableError(callId: number, err: unknown): void {
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
        completeHostCall(callId, 1, bytes);
    } catch (innerErr) {
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
function completeHostCallLastResort(callId: number, err: unknown): void {
    try {
        const msg = HostCallableError.create({
            className: 'InternalError',
            message: `host callable dispatch failed and the error could not be reported: ${safeStringify(err)}`,
            language: 'nodejs',
            category: HostCallableErrorCategory.HOST_CALLABLE_HOST_ERROR,
        });
        const bytes = Buffer.from(HostCallableError.encode(msg).finish());
        completeHostCall(callId, 1, bytes);
    } catch {
        // Nothing more we can safely do; avoid throwing on the libuv loop.
    }
}

/** `String(err)` that cannot itself throw (e.g. a Proxy with a throwing
 * `toString`). */
function safeStringify(err: unknown): string {
    try {
        return String(err);
    } catch {
        return '<unstringifiable error>';
    }
}

function describeError(err: unknown): { className: string; message: string; stack: string | undefined } {
    if (err instanceof Error) {
        return {
            className: err.name || err.constructor.name || 'Error',
            message: err.message || String(err),
            stack: err.stack ?? undefined,
        };
    }
    if (err != null && typeof err === 'object') {
        const ctor = (err as { constructor?: { name?: string } }).constructor;
        const className = ctor?.name && ctor.name !== 'Object' ? ctor.name : 'Error';
        return { className, message: String(err), stack: undefined };
    }
    return { className: 'Error', message: String(err), stack: undefined };
}
