/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
export type LazyEntry = [string, string];
export declare class BamlTypeMap {
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
    getClass(fqn: string): unknown;
    getEnum(fqn: string): unknown;
    getTypeAlias(fqn: string): unknown;
    private _resolve;
    jsTypeToBamlType(cls: unknown): string;
    warm(): void;
}
export declare function setTypeMap(m: BamlTypeMap): void;
export declare function getTypeMap(): BamlTypeMap;
//# sourceMappingURL=typemap.d.ts.map