/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
import { BamlHandle } from './native';
/**
 * Opaque wrapper around a streaming-call handle.
 *
 * `TStream` / `TFinal` are erased at runtime — `BamlStream<TStream, TFinal>` is a
 * compile-time-only generic, the same trade-off as Python's `typing.Generic`.
 *
 * The positional order mirrors the BAML signature `Stream<TStream, TFinal>`.
 */
export declare class BamlStream<TStream, TFinal> {
    private _handle;
    constructor(handle: BamlHandle);
    /** Internal: build a `BamlStream` from a `BamlHandle`. Used by proto decode. */
    static _fromHandle<TStream, TFinal>(handle: BamlHandle): BamlStream<TStream, TFinal>;
    /** Internal: expose the inner `BamlHandle` for inbound encode. */
    _toHandle(): BamlHandle;
    next(): TStream;
    nextAsync(): Promise<TStream>;
    final(): TFinal;
    finalAsync(): Promise<TFinal>;
    private _callSync;
    private _callAsync;
}
//# sourceMappingURL=stream.d.ts.map