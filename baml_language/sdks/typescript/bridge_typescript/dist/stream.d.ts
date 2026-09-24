/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { BamlHandle } from './native.js';
/**
 * A live `ai.stream.Stream<T>`. A partial and the settled value share the one
 * type: a partial is `T` parsed from the text received so far.
 */
export declare class BamlStream<T> {
    private _handle;
    private _classFqn;
    constructor(handle: BamlHandle, classFqn: string);
    /** Internal: produce a fresh BamlStream from a BamlHandle. Used by proto decode. */
    static _fromHandle<T>(handle: BamlHandle, classFqn: string): BamlStream<T>;
    /** Internal: expose the inner BamlHandle for inbound encode. */
    _toHandle(): BamlHandle;
    next(): T;
    nextAsync(): Promise<T>;
    final(): T;
    finalAsync(): Promise<T>;
    private _callSync;
    private _callAsync;
}
//# sourceMappingURL=stream.d.ts.map