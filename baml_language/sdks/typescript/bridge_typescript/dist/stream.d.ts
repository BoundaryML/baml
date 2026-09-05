/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { BamlHandle } from './native.js';
export declare class BamlStream<TStream, TFinal> {
    private readonly _typeMap;
    private _encodeCallArgs;
    private _decodeCallResult;
    private _handle;
    private _classFqn;
    constructor(handle: BamlHandle, classFqn: string);
    /** Internal: produce a fresh BamlStream from a BamlHandle. Used by proto decode. */
    static _fromHandle<TStream, TFinal>(handle: BamlHandle, classFqn: string): BamlStream<TStream, TFinal>;
    /** Internal: expose the inner BamlHandle for inbound encode. */
    _toHandle(): BamlHandle;
    next(): TStream;
    nextAsync(): Promise<TStream>;
    final(): TFinal;
    finalAsync(): Promise<TFinal>;
    private _callSync;
    private _callAsync;
}
//# sourceMappingURL=stream.d.ts.map