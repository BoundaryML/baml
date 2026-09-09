import { baml_bridge } from './proto/baml_cffi.js';
import { BamlEncodedResult, BamlCallContext, HandleKey } from './native.js';
import { BamlTypeMap } from './typemap.js';
import { type BamlTypeValue } from './wire_ty.js';
/**
 * Error thrown when a host callable (a JS `function`) is passed to the
 * *synchronous* call path. See {@link encodeCallArgs} for why this can't work.
 */
export declare class HostCallableSyncError extends Error {
    constructor(message: string);
}
export interface EncodeCallArgsOptions {
    callId: bigint;
    syncMode?: boolean;
    typeMap?: BamlTypeMap;
    functionName?: string;
    functionHandle?: HandleKey;
    concreteMethod?: baml_bridge.cffi.v1.IConcreteMethodTarget;
    /**
     * Call-level TypeVar bindings for a generic function/method, as
     * `[typeVarName, wireTy]` pairs in De Bruijn order (enclosing class params
     * first, then the callee's own `<...>` params). Encoded into
     * `CallFunctionArgs.type_args`. Mirrors Python's `encode_call_args`
     * `type_args` argument. Omitted/empty for non-generic calls.
     */
    typeArgs?: Array<[string, baml_bridge.cffi.v1.IBamlTy | BamlTypeValue]>;
}
export interface BamlPromptCallOptions {
    $ctx?: BamlCallContext;
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
export declare class BamlPrompt {
    private readonly wire;
    private constructor();
    static _fromWire(wire: baml_bridge.cffi.v1.IBamlValuePromptAst): BamlPrompt;
    _wireCopy(): baml_bridge.cffi.v1.IBamlValuePromptAst;
    /** A detached JSON-compatible view of the canonical prompt tree. */
    toJSON(): unknown;
    text(options?: BamlPromptCallOptions): string;
    textAsync(options?: BamlPromptCallOptions): Promise<string>;
    /** Compatibility with the generated SDK's existing async-method spelling. */
    text_async(options?: BamlPromptCallOptions): Promise<string>;
    messages(options?: BamlPromptCallOptions): BamlPromptMessage[];
    messagesAsync(options?: BamlPromptCallOptions): Promise<BamlPromptMessage[]>;
    /** Compatibility with the generated SDK's existing async-method spelling. */
    messages_async(options?: BamlPromptCallOptions): Promise<BamlPromptMessage[]>;
    private _callSync;
    private _callAsync;
    private static cloneWire;
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
 * Callback channels retain their callable without pinning the Node event loop.
 * If a later argument or type token fails, the engine never sees the registered
 * keys or cloned handles, so encoding must release them itself.
 */
export declare function encodeCallArgs(kwargs: Record<string, unknown>, options: EncodeCallArgsOptions): Buffer;
/**
 * Decode a bare `BamlOutboundValue` to a JS value. Used for the host-callable
 * args path, where the engine sends a list-shaped `BamlOutboundValue` rather
 * than the call-result `BamlOutboundResult` envelope.
 */
export declare function decodeOutboundValue(data: Buffer | Uint8Array): unknown;
export declare function decodeCallResult(data: Buffer | Uint8Array | BamlEncodedResult, typeMap?: BamlTypeMap): unknown;
export declare function makeHostCallableDispatch(userFn: (...args: unknown[]) => unknown, typeMap?: BamlTypeMap): (callId: number, argsBytes: BamlEncodedResult) => void;
//# sourceMappingURL=proto.d.ts.map