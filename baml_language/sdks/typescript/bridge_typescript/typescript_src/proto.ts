// proto.ts — mirrors bridge_python/python_src/baml_py/proto.py
//
// Encodes TS objects → CallFunctionArgs protobuf bytes (for sending to Rust)
// Decodes the BamlOutboundResult envelope → TS objects (call results), and
// bare BamlOutboundValue bytes → TS objects (host-callable args).

import { baml_bridge } from './proto/baml_cffi.js';
import {
    BamlHandle,
    HandleKey,
    BamlImage,
    BamlAudio,
    BamlVideo,
    BamlPdf,
    completeHostCallValue,
    completeHostCallThrow,
    completeHostCallRaised,
    completeHostCallBridgeFailure,
    getRuntime,
    newFunctionCall,
    releaseFunctionCall,
    invocationClockNs,
    _releaseWireHandle,
    _traceSelection,
} from './native.js';
import { attachInvocation } from './call_context.js';
import type { InvocationOptions, InvokeOptions } from './invocation.js';
import { BamlStream } from './stream.js';
import { BamlFunctionSpec } from './function_spec.js';
import { BamlAbortError, BamlCancelledError, BamlClientError, BamlError, BamlInvalidArgumentError, BamlPanic, type BamlErrorDetail } from './errors.js';
import { hostMarker } from './host_marker.js';
import { handleExitPanic, runHostCallback, currentExecutionState, observeHostCallbackResult, registerCallableDispatch } from './platform.js';
import {
    registerHostOpaque,
    releaseHostOpaque,
    tryRehydrateHostValueByKey,
} from './host_value_registry.js';
import { BamlTypeMap, getTypeMap } from './typemap.js';
import {
    BamlType,
    BamlTypeMetadataRow,
    liveKeys,
    lowerTypeToWireTy,
    outboundTyToBamlTypeToken,
    type BamlTypeToken,
} from './wire_ty.js';

const CallFunctionArgs = baml_bridge.cffi.v1.CallFunctionArgs;
const BamlOutboundValue = baml_bridge.cffi.v1.BamlOutboundValue;
const BamlOutboundResult = baml_bridge.cffi.v1.BamlOutboundResult;
const BamlToHostCall = baml_bridge.cffi.v1.BamlToHostCall;
const BamlValuePromptAst = baml_bridge.cffi.v1.BamlValuePromptAst;
const InboundValue = baml_bridge.cffi.v1.InboundValue;
const InboundClassValue = baml_bridge.cffi.v1.InboundClassValue;
const InboundMapEntry = baml_bridge.cffi.v1.InboundMapEntry;
const BamlHandleType = baml_bridge.cffi.v1.BamlHandleType;
const CANCELLED_PANIC_CLASS = 'baml.panics.Cancelled';
// Keep engine closures callable in JS while retaining their native identity
// when they cross back into BAML. Wrapping them as host callbacks would enter
// their synchronous JS invocation and block the dispatching event loop.
const bamlClosureHandles = new WeakMap<object, BamlHandle>();

// ─── Inbound (TS → Rust) ───
//
// Argument values are not encoded here. The bridge walks them against the
// types the program declares for the target's parameters (`bridge_encode`),
// so a value that does not fit raises before anything is sent, and every
// owner the walk created (a registered callable, a cloned handle) is released
// again. What the runtime owns, it keeps: the typemap's class lookup, `$types`
// lowering, and the serialization of handles, media, prompts and type values
// are reached through {@link hostAdapter}.

export interface EncodeCallArgsOptions {
    callId: bigint;
    baml?: InvocationOptions | null;
    syncMode?: boolean;
    functionName?: string;
    functionHandle?: HandleKey;
    /**
     * Call-level TypeVar bindings for a generic function/method, as
     * `[typeVarName, wireTy]` pairs in De Bruijn order (enclosing class params
     * first, then the callee's own `<...>` params). Encoded into
     * `CallFunctionArgs.type_args`. Mirrors Python's `encode_call_args`
     * `type_args` argument. Omitted/empty for non-generic calls.
     */
    typeArgs?: Array<[string, baml_bridge.cffi.v1.IBamlTy | BamlType]>;
}

export interface BamlPromptCallOptions {
    $baml?: InvocationOptions | null;
}

/** Structural view returned by `BamlPrompt.messages()`. */
export interface BamlPromptMessage {
    role: string;
    content: string;
    parts: unknown[];
    metadata: Record<string, unknown>;
}

/**
 * Portable representation of `ai.Prompt` at the bridge boundary.
 *
 * The protobuf payload is copied both in and out so this wrapper never owns an
 * engine handle and can safely be passed to another runtime. Its helpers
 * re-enter the canonical `ai.Prompt` methods with a fresh inline copy, so the
 * same prompt remains reusable across repeated calls and runtimes.
 */
export class BamlPrompt {
    private constructor(private readonly wire: baml_bridge.cffi.v1.IBamlValuePromptAst) {}

    static _fromWire(wire: baml_bridge.cffi.v1.IBamlValuePromptAst): BamlPrompt {
        return new BamlPrompt(BamlPrompt.cloneWire(wire));
    }

    _wireCopy(): baml_bridge.cffi.v1.IBamlValuePromptAst {
        return BamlPrompt.cloneWire(this.wire);
    }

    /** A detached JSON-compatible view of the canonical prompt tree. */
    toJSON(): unknown {
        return BamlValuePromptAst.toObject(
            BamlValuePromptAst.create(this._wireCopy()),
            { arrays: true, objects: true },
        );
    }

    text(options?: BamlPromptCallOptions): string {
        return this._callSync('ai.Prompt.text', options) as string;
    }

    async textAsync(options?: BamlPromptCallOptions): Promise<string> {
        return await this._callAsync('ai.Prompt.text', options) as string;
    }

    /** Compatibility with the generated SDK's existing async-method spelling. */
    async text_async(options?: BamlPromptCallOptions): Promise<string> {
        return await this.textAsync(options);
    }

    messages(options?: BamlPromptCallOptions): BamlPromptMessage[] {
        return this._callSync('ai.Prompt.messages', options) as BamlPromptMessage[];
    }

    async messagesAsync(options?: BamlPromptCallOptions): Promise<BamlPromptMessage[]> {
        return await this._callAsync('ai.Prompt.messages', options) as BamlPromptMessage[];
    }

    /** Compatibility with the generated SDK's existing async-method spelling. */
    async messages_async(options?: BamlPromptCallOptions): Promise<BamlPromptMessage[]> {
        return await this.messagesAsync(options);
    }

    private _callSync(fqn: string, options?: BamlPromptCallOptions): unknown {
        const callId = BigInt(newFunctionCall());
        const encoded = encodeCallArgs(
            { self: this },
            { syncMode: true, callId, functionName: fqn, baml: options?.$baml },
        );
        const callCtxBinding = attachInvocation(encoded.bytes, callId);
        try {
            return decodeCallResult(getRuntime().callFunctionSync(encoded.bytes, encoded.kwargs, encoded.cancel));
        } finally {
            callCtxBinding.detach();
        }
    }

    private async _callAsync(fqn: string, options?: BamlPromptCallOptions): Promise<unknown> {
        const callId = BigInt(newFunctionCall());
        const encoded = encodeCallArgs(
            { self: this },
            { callId, functionName: fqn, baml: options?.$baml },
        );
        const callCtxBinding = attachInvocation(encoded.bytes, callId);
        try {
            return decodeCallResult(await getRuntime().callFunction(encoded.bytes, encoded.kwargs, encoded.cancel));
        } finally {
            callCtxBinding.detach();
        }
    }

    private static cloneWire(
        wire: baml_bridge.cffi.v1.IBamlValuePromptAst,
    ): baml_bridge.cffi.v1.IBamlValuePromptAst {
        return BamlValuePromptAst.decode(BamlValuePromptAst.encode(wire).finish());
    }
}

/** Only the decoder mints a {@link BamlReference}. */
const REFERENCE_TOKEN: unique symbol = Symbol('baml.reference');

/**
 * An instance of a standard-library class the engine keeps for itself
 * (`reflect.Package`, `reflect.Session`, …). A host holds it and passes it
 * back, as receiver or argument, but never reads into it or builds one: the
 * generated class extends this one with the class's methods alone, and the
 * decoder makes it from the reference the instance crosses as.
 *
 * One engine instance is one wrapper for as long as the wrapper lives (the
 * decoder reuses it), so two are the same instance exactly when they are the
 * same object.
 */
export class BamlReference {
    private readonly _handle: BamlHandle;
    /**
     * The class's type arguments, by parameter name, for an instance of a
     * generic class: what the engine says the instance is, which an instance
     * method binds the class's parameters from.
     */
    readonly $types: Readonly<Record<string, BamlType>> | undefined;

    // Public only so the class stays a type token (`$types: { T: Package }`):
    // no caller outside this module can name the token, and a forged one throws.
    constructor(
        token: typeof REFERENCE_TOKEN,
        handle: BamlHandle,
        types?: Readonly<Record<string, BamlType>>,
    ) {
        if (token !== REFERENCE_TOKEN) {
            throw new TypeError(`${new.target.name} is the runtime's own; pass the instance you received`);
        }
        this._handle = handle;
        this.$types = types;
    }

    /** Internal: wrap a decoded `ADT_RUNTIME_VALUE` handle. Used by proto decode. */
    static _fromHandle(handle: BamlHandle, types?: Readonly<Record<string, BamlType>>): BamlReference {
        return new this(REFERENCE_TOKEN, handle, types);
    }

    /** Internal: expose the inner handle for inbound encoding. */
    _toHandle(): BamlHandle {
        return this._handle;
    }
}

type BamlMediaValue = BamlImage | BamlAudio | BamlVideo | BamlPdf;

function wireMedia(value: BamlMediaValue): baml_bridge.cffi.v1.IBamlValueMedia {
    const media = value instanceof BamlImage ? 1
        : value instanceof BamlAudio ? 2
        : value instanceof BamlPdf ? 3
        : 4;
    const mimeType = value.mimeType() ?? undefined;
    const url = value.url();
    if (url !== null) return { media, mimeType, url };
    const name = value.name();
    // Content read from a file keeps the file's name.
    if (name !== null) return { media, mimeType, fileContent: { name, base64: value.base64() } };
    return { media, mimeType, base64: value.base64() };
}

/**
 * Generic-class instances declare their TypeVar names (declaration order) in a
 * static `$generic` field that codegen emits; this reads it back. Returns the
 * param-name list for a generic class instance, or `null` for a non-generic
 * one. The value-level type args ride a sibling `$types` instance field.
 */
function genericParamNames(value: object): string[] | null {
    const ctor = (value as { constructor?: { $generic?: unknown } }).constructor;
    const g = ctor?.$generic;
    if (Array.isArray(g) && g.length > 0 && g.every((x) => typeof x === 'string')) {
        return g as string[];
    }
    return null;
}


/**
 * The control part of a call as `CallFunctionArgs` bytes, with the values the
 * bridge encodes against the target's declared types alongside: `kwargs` by
 * declared parameter name and the `$baml.cancel` token, both still JavaScript
 * values. `BamlRuntime.callFunction(bytes, kwargs, cancel)` takes all three.
 */
export interface EncodedCall {
    bytes: Buffer;
    kwargs: Record<string, unknown>;
    cancel: unknown;
}

/**
 * Build the control part of a `CallFunctionArgs` for a call.
 *
 * The argument values are not encoded here; they ride the returned
 * {@link EncodedCall} as JavaScript values, and the bridge walks them against
 * the types the program declares for the target's parameters. `syncMode` is
 * passed to the bridge by the entry point: a host callable in the arguments of
 * a *synchronous* call is rejected there before any work, because the sync
 * path blocks the event loop the callback would need.
 */
export function encodeCallArgs(kwargs: Record<string, unknown>, options: EncodeCallArgsOptions): EncodedCall {
    const callId = options.callId;
    if (callId === 0n) {
        throw new TypeError('callId must be a nonzero uint64');
    }
    try {
        if (options.functionName !== undefined && options.functionHandle !== undefined) {
            throw new TypeError('exactly one BAML call target may be set');
        }
        const snapshot = snapshotInvocation(options.baml, callId);
        // The bridge restates a runtime declaration with a reference of the
        // call's own; the wrapper keeps its rows.
        const typeArgs = (options.typeArgs ?? []).map(([typeVar, value]) => ({
            typeVar,
            typeValue: value instanceof BamlType ? value._wireCopy() : value,
        }));
        const msg = CallFunctionArgs.fromObject({
            callId: callId.toString(),
            typeArgs,
            functionName: options.functionName,
            functionHandle: options.functionHandle,
            invocation: snapshot.wire,
        });
        const bytes = Buffer.from(CallFunctionArgs.encode(msg).finish());
        encodedInvocationOwners.set(bytes, snapshot);
        return { bytes, kwargs: { ...kwargs }, cancel: snapshot.cancel ?? null };
    } catch (err) {
        releaseFunctionCall(callId.toString());
        throw err;
    }
}

/** A leaf's payload as the bridge takes it from {@link hostAdapter.leafOf}: an `InboundValue`. */
interface HostLeaf {
    bytes: Uint8Array;
    /** The handle key cloned for the wire, as a decimal string, so the bridge can release it if the call never goes out. */
    cloned?: string;
}

/** The leaf kinds {@link hostAdapter.leafKindOf} answers, as the bridge numbers them. */
const LEAF_HANDLE = 0;
const LEAF_IMAGE = 1;
const LEAF_AUDIO = 2;
const LEAF_VIDEO = 3;
const LEAF_PDF = 4;
const LEAF_PROMPT = 5;
const LEAF_TYPE_VALUE = 6;

function keyToDecimal(key: HandleKey): string {
    return (BigInt.asUintN(32, BigInt(key.high)) << 32n | BigInt.asUintN(32, BigInt(key.low))).toString();
}

const HOST_VALUE_HANDLE_TYPES: ReadonlySet<number> = new Set([
    baml_bridge.cffi.v1.BamlHandleType.HOST_VALUE_CALLABLE,
    baml_bridge.cffi.v1.BamlHandleType.HOST_VALUE_OPAQUE,
]);

function handleLeaf(handle: BamlHandle): HostLeaf {
    const key = handle._cloneKeyForWire();
    const bytes = InboundValue.encode(InboundValue.create({ handle: { key, handleType: handle.handleType } })).finish();
    // A host value is the engine's reference to the host's own object: its key
    // goes as it is, nothing was cloned and nothing is owed if the call never
    // goes out. An engine handle's clone is the bridge's to release.
    if (HOST_VALUE_HANDLE_TYPES.has(handle.handleType)) return { bytes };
    return { bytes, cloned: keyToDecimal(key) };
}

/** The engine handle a runtime-owned value wraps, if it wraps one. */
function handleOf(value: unknown): BamlHandle | undefined {
    if (value instanceof BamlHandle) return value;
    if (value instanceof BamlStream || value instanceof BamlFunctionSpec || value instanceof BamlReference) return value._toHandle();
    if (typeof value === 'function') return bamlClosureHandles.get(value);
    return undefined;
}

/**
 * What the runtime answers the bridge's walker: everything about a value that
 * only the runtime knows. Registered once at module load
 * (`registerHostAdapter`); the bridge calls back into it while encoding.
 */
/** The getter behind `%TypedArray%.prototype[Symbol.toStringTag]`. */
const typedArrayKind = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(Uint8Array.prototype) as object,
    Symbol.toStringTag,
)!.get as (this: unknown) => string | undefined;

export const hostAdapter = {
    /**
     * Is `value` a plain object: one whose prototype is `Object.prototype` or
     * null? Answered here, in JavaScript, because the question is about realms:
     * an object from another realm (a `vm` context, an iframe) has that realm's
     * `Object.prototype`, which only JavaScript can recognise as one.
     */
    /**
     * Is `value` a `Uint8Array`, whichever realm created it? `instanceof`
     * against this realm's constructor misses a `vm` context's or an iframe's
     * typed array. `%TypedArray%.prototype[Symbol.toStringTag]` reads the
     * value's own kind from its internal slot: `'Uint8Array'` for a typed
     * array of that kind from any realm, `undefined` for anything else, and
     * no own `Symbol.toStringTag` property can spoof it.
     */
    isUint8Array(value: unknown): boolean {
        return typedArrayKind.call(value) === 'Uint8Array';
    },
    isPlainObject(value: unknown): boolean {
        if (value === null || typeof value !== 'object') return false;
        const proto = Object.getPrototypeOf(value);
        if (proto === null || proto === Object.prototype) return true;
        // Another realm's `Object.prototype`: the root of its chain, whose
        // constructor is that realm's `Object`.
        return Object.getPrototypeOf(proto) === null && typeof proto.constructor === 'function' && proto.constructor.name === 'Object';
    },
    /** The BAML class a generated-class instance is an instance of, or undefined. */
    classOf(value: unknown): string | undefined {
        if (value === null || typeof value !== 'object') return undefined;
        const proto = Object.getPrototypeOf(value);
        // An instance of a class no generated SDK covers (a thrown value's,
        // a run-time class's) is a null-prototype object carrying its BAML
        // class under the hidden `$class`, so it crosses back as that class.
        if (proto === null) {
            const named = (value as { $class?: unknown }).$class;
            return typeof named === 'string' && named !== '' ? named : undefined;
        }
        if (proto === Object.prototype) return undefined;
        const fqn = getTypeMap().jsTypeToBamlType((value as object).constructor);
        return fqn === '' ? undefined : fqn;
    },
    /** A generic instance's `$types`, lowered to wire `BamlTy` bytes, when every parameter is present. */
    typeArgsOf(value: unknown): Uint8Array[] | undefined {
        const params = genericParamNames(value as object);
        if (params === null) return undefined;
        const userTypes = (value as { $types?: Record<string, BamlTypeToken | BamlType> }).$types;
        if (!params.every((param) => userTypes?.[param] !== undefined)) return undefined;
        return params.map((param) => {
            const token = userTypes![param];
            const wire = token instanceof BamlType ? token._wireCopy() : lowerTypeToWireTy(token);
            return baml_bridge.cffi.v1.BamlTy.encode(baml_bridge.cffi.v1.BamlTy.create(wire ?? {})).finish();
        });
    },
    /**
     * The kind of leaf a runtime-owned value is, or undefined for any other
     * value. A question only: nothing is cloned or serialized until the bridge
     * asks for the payload with `leafOf`.
     */
    leafKindOf(value: unknown): number | undefined {
        if (value instanceof BamlType) return LEAF_TYPE_VALUE;
        if (value instanceof BamlPrompt) return LEAF_PROMPT;
        if (value instanceof BamlImage) return LEAF_IMAGE;
        if (value instanceof BamlAudio) return LEAF_AUDIO;
        if (value instanceof BamlVideo) return LEAF_VIDEO;
        if (value instanceof BamlPdf) return LEAF_PDF;
        if (handleOf(value) !== undefined) return LEAF_HANDLE;
        return undefined;
    },
    /** The wire payload of a value `leafKindOf` classified as a leaf, cloning a handle's key for the wire. */
    leafOf(value: unknown): HostLeaf {
        if (value instanceof BamlType) {
            return { bytes: InboundValue.encode(InboundValue.create({ tyValue: value._wireCopy() })).finish() };
        }
        if (value instanceof BamlPrompt) {
            return { bytes: InboundValue.encode(InboundValue.create({ promptAstValue: value._wireCopy() })).finish() };
        }
        if (value instanceof BamlImage || value instanceof BamlAudio || value instanceof BamlVideo || value instanceof BamlPdf) {
            return { bytes: InboundValue.encode(InboundValue.create({ mediaValue: wireMedia(value) })).finish() };
        }
        const handle = handleOf(value);
        if (handle !== undefined) return handleLeaf(handle);
        throw new TypeError(`${hostAdapter.describe(value)} is not a leaf`);
    },
    /** Bind a user function as a host callable; what comes back is the platform's (see `registerCallableDispatch`). */
    dispatchFor(fn: (...args: unknown[]) => unknown): unknown {
        return registerCallableDispatch(makeHostCallableDispatch(fn), hostMarker(fn)?.handle);
    },
    /** A short rendering for error messages, bounded in work as in length: a container by its size, never stringified whole. */
    describe(value: unknown): string {
        if (value === null) return 'null';
        if (Array.isArray(value)) return `array of length ${value.length}`;
        if (typeof value === 'object') {
            const name = (value as object).constructor?.name ?? 'object';
            return name === 'Object' ? `object with ${Object.keys(value as object).length} keys` : name;
        }
        if (typeof value === 'function') return 'function';
        if (typeof value === 'symbol') return 'symbol';
        let shown = typeof value === 'string' ? JSON.stringify(value) : String(value);
        if (shown.length > 48) shown = `${shown.slice(0, 48)}…`;
        return `${typeof value} ${shown}`;
    },
};

interface InvocationSnapshot {
    wire: baml_bridge.cffi.v1.IInvocationOptions;
    cancel: unknown;
    signal?: AbortSignal | null;
    retained: unknown[];
}
const encodedInvocationOwners = new WeakMap<Uint8Array, InvocationSnapshot>();
export function encodedInvocation(bytes: Uint8Array): InvocationSnapshot | undefined { return encodedInvocationOwners.get(bytes); }

function snapshotInvocation(options: InvocationOptions | null | undefined, callId: bigint): InvocationSnapshot {
    if (options != null && (typeof options !== 'object' || Array.isArray(options))) throw new TypeError('$baml must be an options object or null');
    const snapshot = { ...(options ?? {}) };
    for (const key of Object.keys(snapshot)) {
        if (!['trace', 'cancel', 'timeoutMs', 'signal'].includes(key)) throw new TypeError(`unknown $baml option ${JSON.stringify(key)}`);
    }
    const wire: baml_bridge.cffi.v1.IInvocationOptions = { inheritedState: currentExecutionState(), hostEnvironment: callId.toString() };
    if (snapshot.timeoutMs != null) {
        if (!Number.isInteger(snapshot.timeoutMs) || snapshot.timeoutMs < 0 || snapshot.timeoutMs > 2147483647) throw new TypeError('timeoutMs must be an integer from 0 through 2147483647');
        const deadline = BigInt(invocationClockNs(callId.toString())) + BigInt(snapshot.timeoutMs) * 1000000n;
        if (deadline > 18446744073709551615n) throw new RangeError('timeout deadline exceeds the runtime clock range');
        wire.deadlineNs = deadline.toString();
    }
    if (snapshot.signal != null && !(snapshot.signal instanceof AbortSignal)) throw new TypeError('signal must be an AbortSignal');
    const retained: unknown[] = [snapshot.cancel];
    const typeMap = getTypeMap();
    if (snapshot.trace != null) {
        const Options = typeMap.getClass('trace.Options') as new (...args: any[]) => object;
        const Reserved = typeMap.getClass('trace.ReservedSpan') as new (...args: any[]) => object;
        if (!(snapshot.trace instanceof Options) && !(snapshot.trace instanceof Reserved)) throw new TypeError('trace must be generated trace.Options or trace.ReservedSpan');
        const handle = (snapshot.trace as { _handle?: unknown })._handle;
        if (!(handle instanceof BamlHandle)) throw new TypeError('trace value must contain a live generated handle');
        const [selection, reservation] = _traceSelection(handle, callId.toString());
        wire.trace = baml_bridge.cffi.v1.TraceSelection.decode(selection);
        retained.push(snapshot.trace, reservation);
    }
    if (snapshot.cancel != null) {
        const Token = typeMap.getClass('baml.spawn.CancelToken') as new (...args: any[]) => object;
        if (!(snapshot.cancel instanceof Token)) throw new TypeError('cancel must be a generated baml.spawn.CancelToken');
    }
    return { wire, cancel: snapshot.cancel, signal: snapshot.signal, retained };
}

export function invokeTarget(target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options: InvokeOptions | undefined, asynchronous: boolean): unknown {
    if (args == null || typeof args !== 'object' || Array.isArray(args)) throw new TypeError('arguments must be a string-keyed object');
    const handle = typeof target === 'function' ? bamlClosureHandles.get(target) : undefined;
    if (typeof target !== 'string' && !handle) throw new TypeError('target must be a BAML name or returned callable');
    const types = options?.$types ?? {};
    if (typeof types !== 'object' || Array.isArray(types)) throw new TypeError('$types must be a binding object');
    if (handle && Object.keys(types).length) throw new TypeError('specialized BAML callable handles do not accept $types');
    const typeArgs: Array<[string, baml_bridge.cffi.v1.IBamlTy | BamlType]> = Object.entries(types).map(([name, type]) => [name, type instanceof BamlType ? type : lowerTypeToWireTy(type)]);
    const callId = BigInt(newFunctionCall());
    const encoded = encodeCallArgs(args, { callId, typeArgs, baml: options?.$baml, syncMode: !asynchronous, ...(handle ? { functionHandle: handle.key } : { functionName: target as string }) });
    const binding = attachInvocation(encoded.bytes, callId);
    if (asynchronous) {
        try { return getRuntime().callFunction(encoded.bytes, encoded.kwargs, encoded.cancel).then(decodeCallResult).finally(() => binding.detach()); }
        catch (error) { binding.detach(); throw error; }
    }
    try { return decodeCallResult(getRuntime().callFunctionSync(encoded.bytes, encoded.kwargs, encoded.cancel)); }
    finally { binding.detach(); }
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
    holder: baml_bridge.cffi.v1.IBamlOutboundValue,
    typeMap: BamlTypeMap,
    ownership?: Set<baml_bridge.cffi.v1.IBamlOutboundHandle>,
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
    if (holder.tyValue) {
        // The wrapper owns each row its type names from here on.
        for (const record of liveRecords.get(holder.tyValue) ?? []) ownership?.delete(record);
        return BamlType._fromWire(holder.tyValue);
    }
    if (holder.classValue) {
        return decodeClass(holder.classValue, typeMap, ownership);
    }
    if (holder.enumValue) {
        return decodeEnum(holder.enumValue, typeMap);
    }
    if (holder.literalValue) {
        // Inline scalars in a oneof (mirrors `BamlTyLiteral`); dispatch on the
        // protobufjs oneof virtual getter (only on the decoded class instance,
        // not the `I`-interface) since scalar fields default to a non-null zero
        // value rather than `null`.
        const lit = holder.literalValue as baml_bridge.cffi.v1.BamlLiteralValue;
        switch (lit.literal) {
            case 'stringValue':
                return lit.stringValue;
            case 'intValue':
                return Number(lit.intValue);
            case 'boolValue':
                return lit.boolValue;
            // Hex / base sixteen on the wire, matching `bigint_value`.
            case 'bigintValue':
                return parseHexBigint(lit.bigintValue ?? '');
            // Source text on the wire (mirrors `BamlTyLiteral.float_value`).
            case 'floatValue':
                return Number(lit.floatValue);
        }
    }
    if (holder.listValue) {
        return (holder.listValue.items || []).map(item => decodeValueHolder(item, typeMap, ownership));
    }
    if (holder.mapValue) {
        const obj: Record<string, unknown> = Object.create(null);
        for (const entry of holder.mapValue.entries || []) {
            if (entry.key != null && entry.value) {
                obj[entry.key] = decodeValueHolder(entry.value, typeMap, ownership);
            }
        }
        return obj;
    }
    if (holder.unionVariantValue && holder.unionVariantValue.value) {
        return decodeValueHolder(holder.unionVariantValue.value, typeMap, ownership);
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
        // The base wrapper owns this occurrence, even if its typed wrapper or
        // an enclosing class constructor subsequently throws.
        ownership?.delete(holder.handleValue);
        if (ht === BamlHandleType.FUNCTION_REF) {
            return decodeBamlClosure(handle, holder.handleValue.ty?.function);
        }
        if (ht === BamlHandleType.ADT_MEDIA_IMAGE) return BamlImage._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_AUDIO) return BamlAudio._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_VIDEO) return BamlVideo._fromHandle(handle);
        if (ht === BamlHandleType.ADT_MEDIA_PDF) return BamlPdf._fromHandle(handle);
        if (ht === BamlHandleType.ADT_FUNCTION_SPEC) {
            return BamlFunctionSpec._fromHandle(handle, 'ai.FunctionSpec');
        }
        if (ht === BamlHandleType.ADT_TAGGED_HEAP_HANDLE) {
            // Dispatch via the typemap: every tagged-heap class self-registers
            // under its engine FQN (codegen emits the entry, e.g.
            // `ai.stream.Stream → BamlStream`), so any class is reachable without
            // special-casing here. Mirrors bridge_python's `_decode_handle`
            // ADT_TAGGED_HEAP_HANDLE arm (sdks/python/.../proto.py).
            // The handle's `ty` is a full `BamlTy`; the typed-wrapper FQN lives
            // on its class variant (a non-class `ty` reads back as `''`).
            const fqn = holder.handleValue.ty?.classTy?.name ?? '';
            const Cls = typeMap.getClass(fqn) as { _fromHandle(h: BamlHandle, classFqn: string): unknown };
            return Cls._fromHandle(handle, fqn);
        }
        if (ht === BamlHandleType.ADT_RUNTIME_VALUE) {
            return decodeRuntimeValue(handle, holder.handleValue.ty, typeMap);
        }
        // ADT_MEDIA_GENERIC has no typed wrapper — stays a bare BamlHandle.
        return handle;
    }
    if (holder.mediaValue) {
        const media = holder.mediaValue;
        const mimeType = media.mimeType ?? undefined;
        const construct = <T>(type: {
            fromUrl(value: string, mimeType?: string): T;
            fromFileContent(file: string, base64: string, mimeType?: string): T;
            fromBase64(value: string, mimeType?: string): T;
        }): T => {
            if (media.url != null) return type.fromUrl(media.url, mimeType);
            if (media.base64 != null) return type.fromBase64(media.base64, mimeType);
            // Content read from a file keeps the file's name.
            if (media.fileContent != null) {
                return type.fromFileContent(media.fileContent.name ?? '', media.fileContent.base64 ?? '', mimeType);
            }
            throw new BamlError('decoded media value has no payload');
        };
        switch (media.media) {
            case 1: return construct(BamlImage);
            case 2: return construct(BamlAudio);
            case 3: return construct(BamlPdf);
            case 4: return construct(BamlVideo);
            default: throw new BamlError(`decoded media value has unsupported kind ${media.media ?? 0}`);
        }
    }
    if (holder.promptAstValue) {
        return BamlPrompt._fromWire(holder.promptAstValue);
    }
    // Any remaining unset oneof is a legitimate null: an all-default holder is a
    // null BAML result.
    return null;
}

/**
 * The reference wrappers alive, by handle key. The engine sends one key per
 * instance, so reusing the live wrapper for a key makes `===` the identity
 * of the engine's instance.
 */
const LIVE_REFERENCES = new Map<string, WeakRef<BamlReference>>();
const FORGET_REFERENCE = new FinalizationRegistry<string>((key) => {
    if (LIVE_REFERENCES.get(key)?.deref() === undefined) {
        LIVE_REFERENCES.delete(key);
    }
});

function referenceKey(handle: BamlHandle): string {
    return `${handle.key.high}:${handle.key.low}`;
}

/**
 * An `ADT_RUNTIME_VALUE` handle. An instance of a class the engine keeps for
 * itself names that class, and becomes its generated wrapper; any other (an
 * instance of a class declared at run time, or one the typemap does not map)
 * stays a bare handle.
 */
function decodeRuntimeValue(
    handle: BamlHandle,
    ty: baml_bridge.cffi.v1.IBamlTy | null | undefined,
    typeMap: BamlTypeMap,
): unknown {
    const fqn = ty?.classTy?.name;
    if (!fqn) return handle;
    let Cls: unknown;
    try {
        Cls = typeMap.getClass(fqn);
    } catch {
        return handle;
    }
    if (typeof Cls === 'function' && Cls.prototype instanceof BamlReference) {
        // A wrapper still alive for this instance is the instance; this
        // handle's ownership is released when the handle is collected.
        const key = referenceKey(handle);
        const live = LIVE_REFERENCES.get(key)?.deref();
        if (live !== undefined) return live;
        const reference = wrapReference(Cls as unknown as typeof BamlReference, handle, ty, fqn);
        LIVE_REFERENCES.set(key, new WeakRef(reference));
        FORGET_REFERENCE.register(reference, key);
        return reference;
    }
    throw new BamlError(`a runtime value of the class ${fqn} has no reference wrapper`);
}

/** The generated wrapper of a reference, with its class's type arguments. */
function wrapReference(
    Cls: typeof BamlReference,
    handle: BamlHandle,
    ty: baml_bridge.cffi.v1.IBamlTy | null | undefined,
    fqn: string,
): BamlReference {
    const Reference = Cls as typeof BamlReference & { $generic?: readonly string[] };
    const params = Reference.$generic;
    if (!params) return Reference._fromHandle(handle);
    // A generic class's instance comes with its type arguments: the class type
    // the engine sends names them, in the class's parameter order.
    const args = ty?.classTy?.typeArgs ?? [];
    if (args.length !== params.length) {
        throw new BamlError(
            `a runtime value of the class ${fqn} came with ${args.length} type argument(s); ` +
            `the class takes ${params.length}`,
        );
    }
    const types: Record<string, BamlType> = {};
    params.forEach((name, index) => {
        types[name] = BamlType._fromWire(args[index]!);
    });
    return Reference._fromHandle(handle, types);
}

function decodeBamlClosure(
    handle: BamlHandle,
    functionTy: baml_bridge.cffi.v1.IBamlTyFunction | null | undefined,
): (...args: unknown[]) => unknown {
    const params = functionTy?.params ?? [];
    const required = params.filter(
        (param) => param.mode !== baml_bridge.cffi.v1.BamlTyFunctionParamMode.BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL,
    );
    const optional = params.filter(
        (param) => param.mode === baml_bridge.cffi.v1.BamlTyFunctionParamMode.BAML_TY_FUNCTION_PARAM_MODE_OPTIONAL,
    );
    const requiredNames = required.map((param, index) => param.name ?? `arg${index}`);
    const optionalNames = new Set(optional.map((param, index) => param.name ?? `arg${required.length + index}`));

    const prepare = (args: unknown[], asynchronous: boolean): unknown => {
        let baml: InvocationOptions | null | undefined;
        if (args.length > requiredNames.length + 1) {
            throw new TypeError(
                `got ${args.length} arguments but this BAML closure accepts ` +
                `${requiredNames.length} required arguments and one optional-arguments object`,
            );
        }
        const kwargs: Record<string, unknown> = {};
        const positionalCount = Math.min(args.length, requiredNames.length);
        for (let index = 0; index < positionalCount; index += 1) {
            kwargs[requiredNames[index]] = args[index];
        }
        if (args.length > requiredNames.length) {
            const options = args[requiredNames.length];
            if (options != null && (Array.isArray(options) || typeof options !== 'object')) {
                throw new TypeError('optional BAML closure arguments must be passed as an object');
            }
            for (const [name, value] of Object.entries((options ?? {}) as Record<string, unknown>)) {
                if (name === '$baml') { baml = value as InvocationOptions | null | undefined; continue; }
                if (name === '$types') {
                    if (value != null && (typeof value !== 'object' || Array.isArray(value) || Object.keys(value).length)) throw new TypeError('specialized BAML callable handles do not accept $types');
                    continue;
                }
                if (!optionalNames.has(name)) {
                    throw new TypeError(`unknown optional argument ${JSON.stringify(name)}`);
                }
                if (value !== undefined) kwargs[name] = value;
            }
        }
        return invokeTarget(closure, kwargs, { $baml: baml }, asynchronous);
    };
    const closure = (...args: unknown[]): unknown => prepare(args, false);
    Object.defineProperty(closure, 'callAsync', { value: async (...args: unknown[]) => await prepare(args, true) });
    bamlClosureHandles.set(closure, handle);
    return closure;
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
    classValue: baml_bridge.cffi.v1.IBamlValueClass,
    typeMap: BamlTypeMap,
    ownership?: Set<baml_bridge.cffi.v1.IBamlOutboundHandle>,
): unknown {
    const fieldDict: Record<string, unknown> = {};
    for (const entry of classValue.fields || []) {
        if (entry.key != null && entry.value) {
            fieldDict[entry.key] = decodeValueHolder(entry.value, typeMap, ownership);
        }
    }
    const fqn = classValue.name ?? '';
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
            const Ctor = Cls as (new (init: Record<string, unknown>) => unknown) & {
                $generic?: readonly string[];
            };
            const instance = new Ctor(fieldDict);
            // Repopulate a generic instance's `$types` from the wire's positional
            // `type_args` (the value-level type channel), keyed by the class's
            // static `$generic` param names — the decode-side mirror of the
            // encoder's sparse `value_type`. Defined non-enumerable so it neither perturbs
            // structural equality (`toEqual`) nor re-encodes as a data field,
            // while still being readable by a later generic instance-method call.
            const params = Array.isArray(Ctor.$generic) ? Ctor.$generic : null;
            const typeArgs = classValue.typeArgs;
            if (params && typeArgs && typeArgs.length) {
                const types: Record<string, BamlTypeToken | undefined> = {};
                params.forEach((p, i) => {
                    types[p] = outboundTyToBamlTypeToken(typeArgs[i]);
                });
                Object.defineProperty(instance, '$types', {
                    value: types,
                    enumerable: false,
                    writable: true,
                    configurable: true,
                });
            }
            return instance;
        }
    }
    // Fallback: a null-prototype object with the fields under their BAML
    // names and the class under the hidden `$class`, so the value keeps its
    // identity when it is sent back (a re-raised panic stays a panic).
    const obj: Record<string, unknown> = Object.create(null);
    for (const [k, v] of Object.entries(fieldDict)) obj[k] = v;
    if (fqn) {
        Object.defineProperty(obj, '$class', { value: fqn, enumerable: false, writable: false, configurable: true });
    }
    return obj;
}

/**
 * Decode an `enum_value` to a typed enum member via the typemap. Falls back to
 * the raw variant string when the FQN is unmapped (bare bridge / unmapped enum).
 */
function decodeEnum(
    enumValue: baml_bridge.cffi.v1.IBamlValueEnum,
    typeMap: BamlTypeMap,
): unknown {
    const fqn = enumValue.name ?? '';
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
    const ownership = new Set<baml_bridge.cffi.v1.IBamlOutboundHandle>();
    collectHostCallHandles(msg, ownership);
    try { return decodeValueHolder(msg, getTypeMap(), ownership); }
    finally { for (const handle of ownership) { try { _releaseWireHandle(handle.key as HandleKey); } catch { /* Preserve decoding failure. */ } } }
}

/**
 * The rows a type value's runtime declarations arrived under, as ownership
 * records, so a failed decode releases them like any other handle.
 */
const liveRecords = new WeakMap<baml_bridge.cffi.v1.IBamlTy, baml_bridge.cffi.v1.IBamlOutboundHandle[]>();

function collectHostCallHandles(
    holder: baml_bridge.cffi.v1.IBamlOutboundValue,
    ownership: Set<baml_bridge.cffi.v1.IBamlOutboundHandle>,
): void {
    if (holder.handleValue) {
        const ht = holder.handleValue.handleType;
        if (ht !== BamlHandleType.HOST_VALUE_CALLABLE && ht !== BamlHandleType.HOST_VALUE_OPAQUE) {
            ownership.add(holder.handleValue);
        }
    } else if (holder.listValue) {
        for (const item of holder.listValue.items ?? []) collectHostCallHandles(item, ownership);
    } else if (holder.mapValue || holder.classValue) {
        const entries = holder.mapValue?.entries ?? holder.classValue?.fields ?? [];
        for (const entry of entries) {
            if (entry.value) collectHostCallHandles(entry.value, ownership);
        }
    } else if (holder.unionVariantValue?.value) {
        collectHostCallHandles(holder.unionVariantValue.value, ownership);
    } else if (holder.tyValue) {
        const records = liveKeys(holder.tyValue).map((key) => ({ key }));
        liveRecords.set(holder.tyValue, records);
        for (const record of records) ownership.add(record);
    }
}

/**
 * Decode the engine→host `BamlToHostCall`. The engine has already resolved the
 * call against the callee's declared params and dropped omitted optionals, so
 * `args` is a flat, declared-order list of the supplied args. Partition it back
 * into the required positional run and the supplied optionals (keyed by
 * `argName`) using each arg's `isOptionalArg` flag.
 */
export function decodeHostCall(data: Buffer | Uint8Array): {
    positional: unknown[];
    optional: Record<string, unknown>;
} {
    const msg = BamlToHostCall.decode(data instanceof Buffer ? data : Buffer.from(data));
    // Track wire objects rather than numeric keys: repeated keys carry one
    // separately owned reference per decoded protobuf occurrence.
    const ownership = new Set<baml_bridge.cffi.v1.IBamlOutboundHandle>();
    for (const arg of msg.args ?? []) {
        if (arg.value) collectHostCallHandles(arg.value, ownership);
    }
    try {
        const typeMap = getTypeMap();
        const positional: unknown[] = [];
        const optional: Record<string, unknown> = {};
        for (const arg of msg.args ?? []) {
            const value = arg.value ? decodeValueHolder(arg.value, typeMap, ownership) : undefined;
            if (arg.isOptionalArg) {
                optional[arg.argName ?? ''] = value;
            } else {
                positional.push(value);
            }
        }
        return { positional, optional };
    } finally {
        for (const handle of ownership) {
            try {
                _releaseWireHandle(handle.key as HandleKey);
            } catch {
                // Cleanup must preserve the original decode failure.
            }
        }
    }
}

/**
 * Reshape the split args into the TypeScript calling convention the callback's
 * generated type advertises: the required args stay positional and the supplied
 * optionals are passed as a single trailing `$opts` object — exactly mirroring
 * how a BAML function is *called* from TypeScript.
 *
 * The `$opts` object is appended only when at least one optional was supplied.
 * With none supplied (or a callback with no optional params at all) the args
 * stay purely positional — the correct shape for a callback declared without an
 * `$opts` object, and the shape that lets the callback's own defaults fill any
 * omitted optionals.
 */
function reshapeHostArgs(positional: unknown[], optional: Record<string, unknown>): unknown[] {
    if (Object.keys(optional).length === 0) {
        return positional;
    }
    return [...positional, optional];
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
/**
 * Peel any `union_variant_value` wrapper(s) so a metadata read sees the inner
 * value. The engine wraps a thrown value in `union_variant_value` when the
 * function declares a multi-member `throws` union; `decodeValueHolder` already
 * unwraps this for the value itself (see the `unionVariantValue` arm), so the
 * FQN read below must match or `className` is lost for union throws.
 */
function unwrapUnionVariant(
    holder: baml_bridge.cffi.v1.IBamlOutboundValue | null | undefined
): baml_bridge.cffi.v1.IBamlOutboundValue | null | undefined {
    let h = holder;
    while (h?.unionVariantValue?.value) {
        h = h.unionVariantValue.value;
    }
    return h;
}

function decodeThrown(
    holder: baml_bridge.cffi.v1.IBamlOutboundValue | null | undefined
): { value: unknown; className: string | undefined; message: string } {
    const className = unwrapUnionVariant(holder)?.classValue?.name ?? undefined;
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

function cancellationAbortError(message: string): BamlAbortError {
    return new BamlAbortError(message, {
        reason: new BamlCancelledError(message),
    });
}

function errorForClassName(formatted: string, detail: BamlErrorDetail): BamlError {
    switch (detail.className) {
        case 'baml.errors.InvalidArgument':
            return new BamlInvalidArgumentError(formatted, detail);
        case 'baml.errors.GenericSdkError':
        case 'baml.errors.CompilationError':
        case 'baml.errors.AccessError':
            return new BamlClientError(formatted, detail);
        default:
            return new BamlError(formatted, detail);
    }
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
            // Same-host rehydration: a `baml.errors.HostCallable` carrying a
            // `_handle` that still resolves in this process's host-value
            // registry re-throws the *original* JS error object the bridge
            // registered on the inbound throw — preserving `raised === caught`
            // identity. Foreign runtimes (a different Node process, the
            // Python bridge) and released keys fall through to the
            // metadata-bearing `BamlError(HostCallable)` wrapper below.
            if (className === 'baml.errors.HostCallable' && value !== null && typeof value === 'object') {
                const handle = (value as Record<string, unknown>)._handle;
                const original = tryRehydrateHostValueByKey(handle);
                if (original !== undefined) {
                    throw original;
                }
            }
            const formatted = formatThrownMessage('error', className ?? '', message, trace);
            if (className === CANCELLED_PANIC_CLASS) {
                throw cancellationAbortError(formatted);
            }
            throw errorForClassName(formatted, { value, bamlTrace: trace, className });
        }
        case 'panic': {
            const panic = result.panic;
            const { value, className, message } = decodeThrown(panic?.value);
            const trace = panic?.trace ?? [];
            const formatted = formatThrownMessage('panic', className ?? '', message, trace);
            if (className === CANCELLED_PANIC_CLASS) {
                throw cancellationAbortError(formatted);
            }
            const fallback = new BamlPanic(
                formatted,
                { value, bamlTrace: trace, className },
            );
            if (panic?.isExitPanic) {
                handleExitPanic(Number(panic.exitCode ?? 0), fallback);
            }
            throw fallback;
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

export function makeHostCallableDispatch(userFn: (...args: unknown[]) => unknown) {
    return (callId: number, argsBytes: Buffer, execution?: object): void => {
        try {
            const completion = runHostCallback(callId, argsBytes, () => dispatchHostCallable(userFn, callId, argsBytes), execution, hostMarker(userFn)?.identity);
            if (completion) {
                void completion.catch((error) => completeHostCallLastResort(callId, error));
            }
        } catch (err) {
            completeHostCallLastResort(callId, err);
        }
    };
}

function dispatchHostCallable(userFn: (...args: unknown[]) => unknown, callId: number, argsBytes: Buffer): void | Promise<void> {
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
            // Host-callable args arrive as a `BamlToHostCall` with a flat
            // `args` list. Partition it into the positional run and the
            // supplied optionals, then reshape into the `$opts` calling
            // convention the callback's type advertises.
            const { positional, optional } = decodeHostCall(argsBytes);
            args = reshapeHostArgs(positional, optional);
        } catch (err) {
            sendHostCallableError(callId, err);
            return;
        }
        let result: unknown;
        try {
            result = userFn(...args);
        } catch (err) {
            observeHostCallbackResult(callId, true, err);
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
            return Promise.resolve(result).then(
                (resolved) => { sendHostCallableResult(callId, resolved); },
                (err) => { observeHostCallbackResult(callId, true, err); sendHostCallableError(callId, err); }
            );
        } else {
            sendHostCallableResult(callId, result);
        }
    } catch (err) {
        // Reached only if a send* helper or the promise plumbing threw
        // *and* did not complete the call. Last-resort completion.
        completeHostCallLastResort(callId, err);
    }
}

function isPromiseLike(value: unknown): value is PromiseLike<unknown> {
    return (
        value != null &&
        (typeof value === 'object' || typeof value === 'function') &&
        typeof (value as { then?: unknown }).then === 'function'
    );
}

function sendHostCallableResult(callId: number, value: unknown): void {
    // The bridge encodes the result against the return type of the slot the
    // callable was bound to; a value that does not fit completes the call as
    // a `HostContractViolation` panic. The trace records what the engine saw,
    // so the outcome is observed after the bridge has decided it. Only an
    // error raised while the value was read is left for the error path here.
    let outcome: string;
    try {
        outcome = completeHostCallValue(callId, value);
    } catch (err) {
        observeHostCallbackResult(callId, true, err);
        sendHostCallableError(callId, err);
        return;
    }
    observeHostCallbackResult(callId, outcome !== 'ok', value);
}

function sendHostCallableError(callId: number, err: unknown): void {
    // Normal error path: never leaves the call uncompleted. If encoding or
    // the native completion itself throws, fall back to the bridge-failure
    // completion below.
    try {
        // A `BamlError(value=<BAML class>)` (or `BamlPanic`, which extends
        // it) throws its value as that real BAML class, encoded against the
        // throws type the callable was bound to, so the BAML caller's typed
        // `catch (e: MyError)` matches. The engine throws a `baml.panics.*`
        // value as a panic whatever the callable declares. A bare wrapper
        // (`value == null`) carries no BAML value and is the host's own
        // exception like any other.
        if (err instanceof BamlError && err.value != null) {
            try {
                completeHostCallThrow(callId, err.value, err instanceof BamlPanic);
                return;
            } catch {
                // Fall through to the host-exception path.
            }
        }

        // The host's own exception: its identity plus a registration of the
        // error object, so the BAML→host decoder on the same Node process can
        // rehydrate the *same* object on round-trip (`raised === caught`).
        // The engine builds the `baml.errors.HostCallable`.
        //
        // Identity-loss edge case (acceptable): `describeError` can itself
        // throw when given a hostile input (e.g. a Proxy whose `constructor`,
        // `name`, or `message` getters throw). Then control jumps to the
        // outer `catch` and the call completes as a bridge failure: the
        // alternative is hanging the call.
        //
        // Register transactionally: if delivery fails before Rust retains the
        // handle, remove the opaque entry before falling back.
        const { className, message, stack } = describeError(err);
        const handleKey = registerHostOpaque(err);
        try {
            completeHostCallRaised(callId, className, message, stack, handleKey);
        } catch (innerErr) {
            releaseHostOpaque(handleKey);
            throw innerErr;
        }
    } catch (innerErr) {
        completeHostCallLastResort(callId, innerErr);
    }
}

/**
 * Absolute last-resort completion: the bridge could not report what the
 * callable did, which is the bridge's failure (`baml.panics.SdkPanic`), with
 * no dependence on the original error object. The only way it can fail is a
 * broken native binding, at which point nothing can complete the call; a
 * throw here is swallowed so it never surfaces as an unhandled rejection on
 * the libuv loop, and the call's lack of completion is the (unavoidable)
 * failure mode.
 */
function completeHostCallLastResort(callId: number, err: unknown): void {
    try {
        completeHostCallBridgeFailure(
            callId,
            `host callable dispatch failed and the error could not be reported: ${safeStringify(err)}`,
        );
    } catch (innerErr) {
        console.error(
            'BAML internal: last-resort host-call completion failed; call will hang.',
            innerErr,
        );
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
        const rawName = err.name;
        const rawMessage = err.message;
        const rawStack = err.stack;
        return {
            className: typeof rawName === 'string' && rawName.length > 0
                ? rawName
                : err.constructor.name || 'Error',
            message: typeof rawMessage === 'string' && rawMessage.length > 0
                ? rawMessage
                : safeStringify(err),
            stack: rawStack == null
                ? undefined
                : typeof rawStack === 'string'
                    ? rawStack
                    : safeStringify(rawStack),
        };
    }
    if (err != null && typeof err === 'object') {
        const ctor = (err as { constructor?: { name?: string } }).constructor;
        const rawName = ctor?.name;
        const className = typeof rawName === 'string' && rawName.length > 0 && rawName !== 'Object'
            ? rawName
            : 'Error';
        return { className, message: safeStringify(err), stack: undefined };
    }
    return { className: 'Error', message: safeStringify(err), stack: undefined };
}
