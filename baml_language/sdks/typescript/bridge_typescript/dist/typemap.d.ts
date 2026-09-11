/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
import { type BamlRuntime } from './native.js';
/** A deferred resolver for a generated class / enum / type alias. */
export type LazyEntry = () => unknown;
export declare class BamlTypeMap {
    runtime?: BamlRuntime;
    private classLazy;
    private enumLazy;
    private aliasLazy;
    private classCache;
    private enumCache;
    private aliasCache;
    private reverse;
    static fromLazyEntries(args: {
        classes: Record<string, LazyEntry>;
        enums: Record<string, LazyEntry>;
        typeAliases: Record<string, LazyEntry>;
    }): BamlTypeMap;
    private _resolve;
    getClass(fqn: string): unknown;
    getEnum(fqn: string): unknown;
    getTypeAlias(fqn: string): unknown;
    /**
     * Reverse lookup for the encode path: given a value's constructor, return
     * its BAML FQN, or "" if it is not a codegen-emitted class. Builds the
     * reverse map lazily by resolving every class/enum thunk once.
     */
    jsTypeToBamlType(ctor: unknown): string;
    warm(): void;
}
export declare function setTypeMap(m: BamlTypeMap, runtime?: BamlRuntime): void;
export declare function getTypeMap(): BamlTypeMap;
/** Only synchronous encode/decode sections use ambient context. Never hold it across await. */
export declare function withTypeMap<T>(m: BamlTypeMap, fn: () => T): T;
export declare function getRuntime(): BamlRuntime;
/** Bind a runtime without changing the SDK's shared nominal type definitions. */
export declare function typeMapForRuntime(runtime: BamlRuntime): BamlTypeMap;
//# sourceMappingURL=typemap.d.ts.map