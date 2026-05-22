/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// stream.ts — pure-TS analog of bridge_python/python_src/baml_py/_stream.py.
//
// BamlStream wraps a BamlHandle whose HANDLE_TABLE row is a
// `CffiHandleTableEntry::Adt(BexExternalAdt::TaggedHeapHandle { ty, heap_handle })`.
// next/final round-trip through getRuntime().callFunction* against the
// well-known FQNs `baml.llm.Stream.next` and `baml.llm.Stream.final`.
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlStream = void 0;
const native_1 = require("./native");
const STREAM_NEXT_FN = 'baml.llm.Stream.next';
const STREAM_FINAL_FN = 'baml.llm.Stream.final';
// `proto.ts` imports `BamlStream` at module load. To avoid a circular
// import, the call-path helpers (`encodeCallArgs`, `decodeCallResult`) are
// imported lazily inside `_callSync` / `_callAsync`.
/**
 * Opaque wrapper around a streaming-call handle.
 *
 * `TStream` / `TFinal` are erased at runtime — `BamlStream<TStream, TFinal>` is a
 * compile-time-only generic, the same trade-off as Python's `typing.Generic`.
 *
 * The positional order mirrors the BAML signature `Stream<TStream, TFinal>`.
 */
// eslint-disable-next-line @typescript-eslint/no-unused-vars
class BamlStream {
    constructor(handle) {
        this._handle = handle;
    }
    /** Internal: build a `BamlStream` from a `BamlHandle`. Used by proto decode. */
    static _fromHandle(handle) {
        return new BamlStream(handle);
    }
    /** Internal: expose the inner `BamlHandle` for inbound encode. */
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
        // Lazy import to break the circular dependency proto.ts → stream.ts.
        const { encodeCallArgs, decodeCallResult } = require('./proto');
        const rt = (0, native_1.getRuntime)();
        const argsProto = encodeCallArgs({ self: this });
        const resultBytes = rt.callFunctionSync(fqn, argsProto, null, null, null);
        return decodeCallResult(resultBytes);
    }
    async _callAsync(fqn) {
        const { encodeCallArgs, decodeCallResult } = require('./proto');
        const rt = (0, native_1.getRuntime)();
        const argsProto = encodeCallArgs({ self: this });
        const resultBytes = await rt.callFunction(fqn, argsProto, null, null, null);
        return decodeCallResult(resultBytes);
    }
}
exports.BamlStream = BamlStream;
//# sourceMappingURL=stream.js.map