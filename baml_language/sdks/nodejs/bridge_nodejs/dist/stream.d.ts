/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import { BamlHandle } from './native.js';
export declare class BamlStream<TStream, TFinal> {
    private _handle;
    constructor(handle: BamlHandle);
    /** Internal: produce a fresh BamlStream from a BamlHandle. Used by proto decode. */
    static _fromHandle<TStream, TFinal>(handle: BamlHandle): BamlStream<TStream, TFinal>;
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