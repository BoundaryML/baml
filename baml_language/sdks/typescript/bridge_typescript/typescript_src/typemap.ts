// typemap.ts — runtime BamlTypeMap, the Node analog of
// sdks/python/src/baml_bridge/typemap.py.
//
// Codegen emits `_typemap.ts` with `BamlTypeMap.fromLazyEntries({ classes,
// enums, typeAliases })` where each entry is a resolver thunk over a statically
// imported generated namespace, e.g. `() => __leaf_0.Resume`. Resolution happens
// in the SDK's module scope (the runtime package can't resolve a
// `baml_sdk/...` path). The root `index.ts` calls `setTypeMap(_TYPE_MAP)` at
// import time; resolution is lazy and memoized on first lookup.

import { getRuntime as nativeGetRuntime, type BamlRuntime } from './native.js';
import { BamlError } from './errors.js';

/** A deferred resolver for a generated class / enum / type alias. */
export type LazyEntry = () => unknown;

export class BamlTypeMap {
    runtime?: BamlRuntime;
    private classLazy = new Map<string, LazyEntry>();
    private enumLazy = new Map<string, LazyEntry>();
    private aliasLazy = new Map<string, LazyEntry>();
    private classCache = new Map<string, unknown>();
    private enumCache = new Map<string, unknown>();
    private aliasCache = new Map<string, unknown>();
    // Reverse map (constructor or enum-object identity → FQN) for the encode path. Lazily
    // built from the class/enum thunks on first `jsTypeToBamlType` call. The
    // five stdlib media/stream wrappers encode via `instanceof` in proto.ts,
    // so they don't need to be seeded here.
    private reverse: Map<unknown, string> | null = null;

    static fromLazyEntries(args: {
        classes: Record<string, LazyEntry>;
        enums: Record<string, LazyEntry>;
        typeAliases: Record<string, LazyEntry>;
    }): BamlTypeMap {
        const m = new BamlTypeMap();
        for (const [fqn, le] of Object.entries(args.classes)) m.classLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.enums)) m.enumLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.typeAliases)) m.aliasLazy.set(fqn, le);
        return m;
    }

    private _resolve(
        fqn: string,
        lazy: Map<string, LazyEntry>,
        cache: Map<string, unknown>,
        kind: string,
    ): unknown {
        if (cache.has(fqn)) return cache.get(fqn);
        const thunk = lazy.get(fqn);
        if (thunk === undefined) throw new BamlError(`Unknown ${kind} FQN ${fqn}`);
        let resolved: unknown;
        try {
            resolved = thunk();
        } catch (e) {
            throw new BamlError(`Failed to resolve ${kind} ${fqn}: ${String(e)}`);
        }
        if (resolved === undefined) {
            throw new BamlError(`Could not resolve ${kind} ${fqn} (resolver returned undefined)`);
        }
        cache.set(fqn, resolved);
        return resolved;
    }

    getClass(fqn: string): unknown {
        return this._resolve(fqn, this.classLazy, this.classCache, 'class');
    }

    getEnum(fqn: string): unknown {
        return this._resolve(fqn, this.enumLazy, this.enumCache, 'enum');
    }

    getTypeAlias(fqn: string): unknown {
        return this._resolve(fqn, this.aliasLazy, this.aliasCache, 'type alias');
    }

    /**
     * Reverse lookup for the encode path: given a value's constructor, return
     * its BAML FQN, or "" if it is not a codegen-emitted class. Builds the
     * reverse map lazily by resolving every class/enum thunk once.
     */
    jsTypeToBamlType(ctor: unknown): string {
        if (this.reverse === null) {
            this.reverse = new Map();
            for (const [fqn, thunk] of this.classLazy) {
                try {
                    this.reverse.set(thunk(), fqn);
                } catch {
                    /* unresolvable entry — skip */
                }
            }
            for (const [fqn, thunk] of this.enumLazy) {
                try {
                    this.reverse.set(thunk(), fqn);
                } catch {
                    /* unresolvable entry — skip */
                }
            }
        }
        let cur: unknown = ctor;
        while (cur != null) {
            const fqn = this.reverse.get(cur);
            if (fqn !== undefined) return fqn;
            cur = Object.getPrototypeOf(cur);
        }
        return '';
    }

    warm(): void {
        for (const k of this.classLazy.keys()) this.getClass(k);
        for (const k of this.enumLazy.keys()) this.getEnum(k);
        for (const k of this.aliasLazy.keys()) this.getTypeAlias(k);
    }
}

let _TYPE_MAP = new BamlTypeMap();
let activeTypeMap: BamlTypeMap | undefined;
export function setTypeMap(m: BamlTypeMap, runtime?: BamlRuntime): void {
    m.runtime = runtime;
    if (!runtime) _TYPE_MAP = m;
}
export function getTypeMap(): BamlTypeMap { return activeTypeMap ?? _TYPE_MAP; }

/** Only synchronous encode/decode sections use ambient context. Never hold it across await. */
export function withTypeMap<T>(m: BamlTypeMap, fn: () => T): T {
    const previous = activeTypeMap;
    activeTypeMap = m;
    try { return fn(); } finally { activeTypeMap = previous; }
}
export function getRuntime(): BamlRuntime { return getTypeMap().runtime ?? nativeGetRuntime(); }

/** Bind a runtime without changing the SDK's shared nominal type definitions. */
export function typeMapForRuntime(runtime: BamlRuntime): BamlTypeMap {
    const base = getTypeMap();
    if (base.runtime?.runtimeKey === runtime.runtimeKey) return base;
    const bound: BamlTypeMap = Object.create(base);
    bound.runtime = runtime;
    return bound;
}
