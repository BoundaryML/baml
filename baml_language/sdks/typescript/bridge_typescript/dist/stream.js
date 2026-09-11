/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { getRuntime, getTypeMap, withTypeMap } from './typemap.js';
// stream.ts — pure-TS analog of sdks/python/src/baml_bridge/_stream.py.
//
// BamlStream wraps a BamlHandle whose HANDLE_TABLE row is a
// `CffiHandleTableEntry::Adt(BexExternalAdt::TaggedHeapHandle { ty, heap_handle })`
// (handle_type ADT_TAGGED_HEAP_HANDLE). next/final round-trip through
// (this._typeMap.runtime ?? getRuntime()).callFunction* against methods on the class FQN carried by
// that tagged handle.
//
// The runtime exports this under its `BamlStream` name; codegen aliases it as
// `Stream` on re-export (`export { BamlStream as Stream } from ...`).
//
// The per-chunk `next`/`final` pulls here are independent of whether the host
// obtained the stream through `Fn$stream` or `Fn$stream_async`. Those bindings
// send the authored FQN with the Stream boundary operation; the engine resolves
// PPIR's private `Fn@stream`. The wrapper exposes both sync and async pulls.
import { newFunctionCall as nativeNewFunctionCall } from './native.js';
import { supportsSyncStreamPulls } from './platform.js';
import { encodeCallArgs, decodeCallResult } from './proto.js';
function newFunctionCall() {
    return BigInt(nativeNewFunctionCall());
}
export class BamlStream {
    _typeMap = getTypeMap();
    _encodeCallArgs(...args) { return withTypeMap(this._typeMap, () => encodeCallArgs(...args)); }
    _decodeCallResult(...args) { return withTypeMap(this._typeMap, () => decodeCallResult(...args)); }
    _handle;
    _classFqn;
    constructor(handle, classFqn) {
        if (classFqn.length === 0) {
            throw new Error('a BAML stream handle must carry its class FQN');
        }
        this._handle = handle;
        this._classFqn = classFqn;
    }
    /** Internal: produce a fresh BamlStream from a BamlHandle. Used by proto decode. */
    static _fromHandle(handle, classFqn) {
        return new BamlStream(handle, classFqn);
    }
    /** Internal: expose the inner BamlHandle for inbound encode. */
    _toHandle() {
        return this._handle;
    }
    next() {
        return this._callSync(`${this._classFqn}.next`);
    }
    async nextAsync() {
        return (await this._callAsync(`${this._classFqn}.next`));
    }
    final() {
        return this._callSync(`${this._classFqn}.final`);
    }
    async finalAsync() {
        return (await this._callAsync(`${this._classFqn}.final`));
    }
    _callSync(fqn) {
        if (!supportsSyncStreamPulls) {
            throw new Error('synchronous stream pulls are unavailable in Web runtimes; use nextAsync() or finalAsync() instead');
        }
        const rt = this._typeMap.runtime ?? getRuntime();
        const argsProto = this._encodeCallArgs({ self: this }, { syncMode: true, callId: newFunctionCall(), functionName: fqn });
        const resultBytes = rt.callFunctionSync(argsProto, null, null);
        return this._decodeCallResult(resultBytes);
    }
    async _callAsync(fqn) {
        const rt = this._typeMap.runtime ?? getRuntime();
        const argsProto = this._encodeCallArgs({ self: this }, { callId: newFunctionCall(), functionName: fqn });
        const resultBytes = await rt.callFunction(argsProto, null, null);
        return this._decodeCallResult(resultBytes);
    }
}
//# sourceMappingURL=stream.js.map