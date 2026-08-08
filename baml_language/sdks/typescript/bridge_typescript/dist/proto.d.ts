/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { baml_bridge } from './proto/baml_cffi.js';
import { HandleKey } from './native.js';
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
    /**
     * Call-level TypeVar bindings for a generic function/method, as
     * `[typeVarName, wireTy]` pairs in De Bruijn order (enclosing class params
     * first, then the callee's own `<...>` params). Encoded into
     * `CallFunctionArgs.type_args`. Mirrors Python's `encode_call_args`
     * `type_args` argument. Omitted/empty for non-generic calls.
     */
    typeArgs?: Array<[string, baml_bridge.cffi.v1.IBamlTy | BamlType]>;
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