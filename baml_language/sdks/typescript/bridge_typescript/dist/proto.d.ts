/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { baml_bridge } from './proto/baml_cffi.js';
import { HandleKey } from './native.js';
import type { InvocationOptions, InvokeOptions } from './invocation.js';
import { BamlType } from './wire_ty.js';
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
export declare function encodeCallArgs(kwargs: Record<string, unknown>, options: EncodeCallArgsOptions): EncodedCall;
/** A leaf's payload as the bridge takes it from {@link hostAdapter.leafOf}: an `InboundValue`. */
interface HostLeaf {
    bytes: Uint8Array;
    /** The handle key cloned for the wire, as a decimal string, so the bridge can release it if the call never goes out. */
    cloned?: string;
}
/**
 * What the runtime answers the bridge's walker: everything about a value that
 * only the runtime knows. Registered once at module load
 * (`registerHostAdapter`); the bridge calls back into it while encoding.
 */
export declare const hostAdapter: {
    /** The BAML class a generated-class instance is an instance of, or undefined. */
    classOf(value: unknown): string | undefined;
    /** A generic instance's `$types`, lowered to wire `BamlTy` bytes, when every parameter is present. */
    typeArgsOf(value: unknown): Uint8Array[] | undefined;
    /**
     * The kind of leaf a runtime-owned value is, or undefined for any other
     * value. A question only: nothing is cloned or serialized until the bridge
     * asks for the payload with `leafOf`.
     */
    leafKindOf(value: unknown): number | undefined;
    /** The wire payload of a value `leafKindOf` classified as a leaf, cloning a handle's key for the wire. */
    leafOf(value: unknown): HostLeaf;
    /** Bind a user function as a host callable; what comes back is the platform's (see `registerCallableDispatch`). */
    dispatchFor(fn: (...args: unknown[]) => unknown): unknown;
    /** A short rendering for error messages, bounded in work as in length: a container by its size, never stringified whole. */
    describe(value: unknown): string;
};
interface InvocationSnapshot {
    wire: baml_bridge.cffi.v1.IInvocationOptions;
    cancel: unknown;
    signal?: AbortSignal | null;
    retained: unknown[];
}
export declare function encodedInvocation(bytes: Uint8Array): InvocationSnapshot | undefined;
export declare function invokeTarget(target: string | ((...args: any[]) => unknown), args: Record<string, unknown>, options: InvokeOptions | undefined, asynchronous: boolean): unknown;
/**
 * Decode a bare `BamlOutboundValue` to a JS value. Used for the host-callable
 * args path, where the engine sends a list-shaped `BamlOutboundValue` rather
 * than the call-result `BamlOutboundResult` envelope.
 */
export declare function decodeOutboundValue(data: Buffer | Uint8Array): unknown;
/**
 * Decode the engine→host `BamlToHostCall`. The engine has already resolved the
 * call against the callee's declared params and dropped omitted optionals, so
 * `args` is a flat, declared-order list of the supplied args. Partition it back
 * into the required positional run and the supplied optionals (keyed by
 * `argName`) using each arg's `isOptionalArg` flag.
 */
export declare function decodeHostCall(data: Buffer | Uint8Array): {
    positional: unknown[];
    optional: Record<string, unknown>;
};
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
export declare function makeHostCallableDispatch(userFn: (...args: unknown[]) => unknown): (callId: number, argsBytes: Buffer, execution?: object) => void;
export {};
//# sourceMappingURL=proto.d.ts.map