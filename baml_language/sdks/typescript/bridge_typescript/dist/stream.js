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
// getOrInitRuntime().callFunction* against methods on the class FQN carried by
// that tagged handle.
//
// The runtime exports this under its `BamlStream` name; codegen aliases it as
// `Stream` on re-export (`export { BamlStream as Stream } from ...`).
//
// The per-chunk `next`/`final` pulls here are independent of whether the host
// obtained the stream through `Fn$stream` or `Fn$stream_async`. Those bindings
// send the authored FQN with the Stream boundary operation; the engine resolves
// PPIR's private `Fn@stream`. The wrapper exposes both sync and async pulls.
import { supportsSyncStreamPulls } from './platform.js';
import { invokeTarget } from './proto.js';
/**
 * A live `ai.stream.Stream<T>`. A partial and the settled value share the one
 * type: a partial is `T` parsed from the text received so far.
 */
export class BamlStream {
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
    next(options) {
        return this._callSync(`${this._classFqn}.next`, options);
    }
    async nextAsync(options) {
        return (await this._callAsync(`${this._classFqn}.next`, options));
    }
    final(options) {
        return this._callSync(`${this._classFqn}.final`, options);
    }
    async finalAsync(options) {
        return (await this._callAsync(`${this._classFqn}.final`, options));
    }
    _callSync(fqn, options) {
        if (!supportsSyncStreamPulls) {
            throw new Error('synchronous stream pulls are unavailable in Web runtimes; use nextAsync() or finalAsync() instead');
        }
        return invokeTarget(fqn, { self: this }, options, false);
    }
    async _callAsync(fqn, options) {
        return await invokeTarget(fqn, { self: this }, options, true);
    }
}
//# sourceMappingURL=stream.js.map