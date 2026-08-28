/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { baml_bridge } from './proto/baml_cffi.js';
import { BamlCallContext, HandleKey } from './native.js';
import { BamlType } from './wire_ty.js';
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
    functionName?: string;
    functionHandle?: HandleKey;
    /** Bridge operation for an authored BAML function. Omitted means the
     * backwards-compatible direct call (wire value 0). */
    operation?: FunctionOperation;
    /**
     * Call-level TypeVar bindings for a generic function/method, as
     * `[typeVarName, wireTy]` pairs in De Bruijn order (enclosing class params
     * first, then the callee's own `<...>` params). Encoded into
     * `CallFunctionArgs.type_args`. Mirrors Python's `encode_call_args`
     * `type_args` argument. Omitted/empty for non-generic calls.
     */
    typeArgs?: Array<[string, baml_bridge.cffi.v1.IBamlTy | BamlType]>;
}
export type FunctionOperation = 'direct' | 'spec' | 'stream';
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
 * Because the Node tsfn is built with `weak::<false>` it keeps a strong libuv
 * ref, so a *leaked* registry entry can also keep the Node process from
 * exiting — which is exactly why the encode-error rollback below matters: if a
 * later kwarg fails, the engine never sees (and so never releases) the keys we
 * already registered, so we release them here.
 */
export declare function encodeCallArgs(kwargs: Record<string, unknown>, options: EncodeCallArgsOptions): Buffer;
/**
 * Decode a bare `BamlOutboundValue` to a JS value. Used for the host-callable
 * args path, where the engine sends a list-shaped `BamlOutboundValue` rather
 * than the call-result `BamlOutboundResult` envelope.
 */
export declare function decodeOutboundValue(data: Buffer | Uint8Array): unknown;
/**
 * Decode a `BamlOutboundResult` envelope (the engine's call-result wire shape
 * after 31c/31e). The `ok` arm returns the decoded value; the `error`/`panic`
 * arms **throw** a `BamlError`/`BamlPanic` carrying the fully decoded thrown
 * value (`.value`), the BAML trace (`.bamlTrace`), and the class FQN
 * (`.className`), with a readable formatted `.message`. An `is_exit_panic`
 * (clean `baml.sys.exit`) terminates the process via `process.exit(code)`
 * rather than throwing.
 */
export declare function decodeCallResult(data: Buffer | Uint8Array): unknown;
export declare function makeHostCallableDispatch(userFn: (...args: unknown[]) => unknown): (callId: number, argsBytes: Buffer) => void;
//# sourceMappingURL=proto.d.ts.map