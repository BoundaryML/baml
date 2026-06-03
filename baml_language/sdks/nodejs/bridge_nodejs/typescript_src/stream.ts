// stream.ts — pure-TS analog of sdks/python/src/baml_core/_stream.py.
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

import { BamlHandle, getRuntime } from './native.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';

const STREAM_NEXT_FN = 'baml.llm.Stream.next';
const STREAM_FINAL_FN = 'baml.llm.Stream.final';

export class BamlStream<TStream, TFinal> {
    private _handle: BamlHandle;

    constructor(handle: BamlHandle) {
        this._handle = handle;
    }

    /** Internal: produce a fresh BamlStream from a BamlHandle. Used by proto decode. */
    static _fromHandle<TStream, TFinal>(handle: BamlHandle): BamlStream<TStream, TFinal> {
        return new BamlStream<TStream, TFinal>(handle);
    }

    /** Internal: expose the inner BamlHandle for inbound encode. */
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
        const rt = getRuntime();
        const argsProto = encodeCallArgs({ self: this }, /* syncMode */ true);
        const resultBytes = rt.callFunctionSync(fqn, argsProto, null, null, null);
        return decodeCallResult(resultBytes);
    }
    private async _callAsync(fqn: string): Promise<unknown> {
        const rt = getRuntime();
        const argsProto = encodeCallArgs({ self: this });
        const resultBytes = await rt.callFunction(fqn, argsProto, null, null, null);
        return decodeCallResult(resultBytes);
    }
}
