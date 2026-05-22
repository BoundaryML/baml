// stream.ts — pure-TS analog of bridge_python/python_src/baml_py/_stream.py.
//
// BamlStream wraps a BamlHandle whose HANDLE_TABLE row is a
// `CffiHandleTableEntry::Adt(BexExternalAdt::TaggedHeapHandle { ty, heap_handle })`.
// next/final round-trip through getRuntime().callFunction* against the
// well-known FQNs `baml.llm.Stream.next` and `baml.llm.Stream.final`.

import { BamlHandle, getRuntime } from './native';

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
export class BamlStream<TStream, TFinal> {
    private _handle: BamlHandle;

    constructor(handle: BamlHandle) {
        this._handle = handle;
    }

    /** Internal: build a `BamlStream` from a `BamlHandle`. Used by proto decode. */
    static _fromHandle<TStream, TFinal>(handle: BamlHandle): BamlStream<TStream, TFinal> {
        return new BamlStream<TStream, TFinal>(handle);
    }

    /** Internal: expose the inner `BamlHandle` for inbound encode. */
    _toHandle(): BamlHandle {
        return this._handle;
    }

    next(): TStream {
        return this._callSync(STREAM_NEXT_FN) as TStream;
    }

    async nextAsync(): Promise<TStream> {
        return (await this._callAsync(STREAM_NEXT_FN)) as TStream;
    }

    final(): TFinal {
        return this._callSync(STREAM_FINAL_FN) as TFinal;
    }

    async finalAsync(): Promise<TFinal> {
        return (await this._callAsync(STREAM_FINAL_FN)) as TFinal;
    }

    private _callSync(fqn: string): unknown {
        // Lazy import to break the circular dependency proto.ts → stream.ts.
        const { encodeCallArgs, decodeCallResult } = require('./proto');
        const rt = getRuntime();
        const argsProto = encodeCallArgs({ self: this });
        const resultBytes = rt.callFunctionSync(fqn, argsProto, null, null, null);
        return decodeCallResult(resultBytes);
    }

    private async _callAsync(fqn: string): Promise<unknown> {
        const { encodeCallArgs, decodeCallResult } = require('./proto');
        const rt = getRuntime();
        const argsProto = encodeCallArgs({ self: this });
        const resultBytes = await rt.callFunction(fqn, argsProto, null, null, null);
        return decodeCallResult(resultBytes);
    }
}
