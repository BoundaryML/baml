/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// stream.ts — pure-TS analog of sdks/python/src/baml_bridge/_stream.py.
//
// BamlStream wraps a BamlHandle whose HANDLE_TABLE row is a
// `CffiHandleTableEntry::Adt(BexExternalAdt::TaggedHeapHandle { ty, heap_handle })`
// (handle_type ADT_TAGGED_HEAP_HANDLE). next/final round-trip through
// getRuntime().callFunction* against the well-known FQNs
// `baml.llm.Stream.next` and `baml.llm.Stream.final`.
//
// The runtime exports this under its `BamlStream` name; codegen aliases it as
// `Stream` on re-export (`export { BamlStream as Stream } from ...`).
//
// Spec note: at the codegen layer only async streaming is a real call
// (`$stream_async`); the per-chunk `next`/`final` pulls here are unrelated to
// that function-level distinction. The wrapper exposes both sync and async
// pulls, as Python does.
import { getRuntime, newFunctionCall as nativeNewFunctionCall } from './native.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
const STREAM_NEXT_FN = 'baml.llm.Stream.next';
const STREAM_FINAL_FN = 'baml.llm.Stream.final';
function newFunctionCall() {
    return BigInt(nativeNewFunctionCall());
}
export class BamlStream {
    _handle;
    constructor(handle) {
        this._handle = handle;
    }
    /** Internal: produce a fresh BamlStream from a BamlHandle. Used by proto decode. */
    static _fromHandle(handle) {
        return new BamlStream(handle);
    }
    /** Internal: expose the inner BamlHandle for inbound encode. */
    _toHandle() {
        return this._handle;
    }
    next() {
        return this._callSync(STREAM_NEXT_FN);
    }
    async nextAsync() {
        return (await this._callAsync(STREAM_NEXT_FN));
    }
    final() {
        return this._callSync(STREAM_FINAL_FN);
    }
    async finalAsync() {
        return (await this._callAsync(STREAM_FINAL_FN));
    }
    _callSync(fqn) {
        const rt = getRuntime();
        const argsProto = encodeCallArgs({ self: this }, { syncMode: true, callId: newFunctionCall() });
        const resultBytes = rt.callFunctionSync(fqn, argsProto, null, null);
        return decodeCallResult(resultBytes);
    }
    async _callAsync(fqn) {
        const rt = getRuntime();
        const argsProto = encodeCallArgs({ self: this }, { callId: newFunctionCall() });
        const resultBytes = await rt.callFunction(fqn, argsProto, null, null);
        return decodeCallResult(resultBytes);
    }
}
//# sourceMappingURL=stream.js.map