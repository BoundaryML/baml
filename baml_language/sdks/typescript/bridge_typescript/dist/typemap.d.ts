/** A deferred resolver for a generated class / enum / type alias. */
export type LazyEntry = () => unknown;
export declare class BamlTypeMap {
    private classLazy;
    private enumLazy;
    private aliasLazy;
    private interfaceLazy;
    private classCache;
    private enumCache;
    private aliasCache;
    private interfaceCache;
    private reverse;
    static fromLazyEntries(args: {
        classes: Record<string, LazyEntry>;
        enums: Record<string, LazyEntry>;
        typeAliases: Record<string, LazyEntry>;
        interfaces?: Record<string, LazyEntry>;
    }): BamlTypeMap;
    private _resolve;
    getClass(fqn: string): unknown;
    getEnum(fqn: string): unknown;
    getTypeAlias(fqn: string): unknown;
    getInterface(fqn: string): unknown;
    /**
     * Reverse lookup for the encode path: given a value's constructor, return
     * its BAML FQN, or "" if it is not a codegen-emitted class. Builds the
     * reverse map lazily by resolving every class/enum thunk once.
     */
    jsTypeToBamlType(ctor: unknown): string;
    warm(): void;
}
export declare function setTypeMap(m: BamlTypeMap): void;
export declare function getTypeMap(): BamlTypeMap;
//# sourceMappingURL=typemap.d.ts.map