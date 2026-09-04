/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/sdks/typescript/bridge_typescript/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/*.proto
 * Build:  cd baml_language/sdks/typescript/bridge_typescript && pnpm build:debug
 */
// typemap.ts — runtime BamlTypeMap, the Node analog of
// sdks/python/src/baml_bridge/typemap.py.
//
// Codegen emits `_typemap.ts` with `BamlTypeMap.fromLazyEntries({ classes,
// enums, typeAliases })` where each entry is a resolver thunk over a statically
// imported generated namespace, e.g. `() => __leaf_0.Resume`. Resolution happens
// in the SDK's module scope (the runtime package can't resolve a
// `baml_sdk/...` path). The root `index.ts` calls `setTypeMap(_TYPE_MAP)` at
// import time; resolution is lazy and memoized on first lookup.
import { getRuntime as nativeGetRuntime } from './native.js';
import { BamlError } from './errors.js';
export class BamlTypeMap {
    runtime;
    classLazy = new Map();
    enumLazy = new Map();
    aliasLazy = new Map();
    classCache = new Map();
    enumCache = new Map();
    aliasCache = new Map();
    // Reverse map (constructor or enum-object identity → FQN) for the encode path. Lazily
    // built from the class/enum thunks on first `jsTypeToBamlType` call. The
    // five stdlib media/stream wrappers encode via `instanceof` in proto.ts,
    // so they don't need to be seeded here.
    reverse = null;
    static fromLazyEntries(args) {
        const m = new BamlTypeMap();
        for (const [fqn, le] of Object.entries(args.classes))
            m.classLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.enums))
            m.enumLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.typeAliases))
            m.aliasLazy.set(fqn, le);
        return m;
    }
    _resolve(fqn, lazy, cache, kind) {
        if (cache.has(fqn))
            return cache.get(fqn);
        const thunk = lazy.get(fqn);
        if (thunk === undefined)
            throw new BamlError(`Unknown ${kind} FQN ${fqn}`);
        let resolved;
        try {
            resolved = thunk();
        }
        catch (e) {
            throw new BamlError(`Failed to resolve ${kind} ${fqn}: ${String(e)}`);
        }
        if (resolved === undefined) {
            throw new BamlError(`Could not resolve ${kind} ${fqn} (resolver returned undefined)`);
        }
        cache.set(fqn, resolved);
        return resolved;
    }
    getClass(fqn) {
        return this._resolve(fqn, this.classLazy, this.classCache, 'class');
    }
    getEnum(fqn) {
        return this._resolve(fqn, this.enumLazy, this.enumCache, 'enum');
    }
    getTypeAlias(fqn) {
        return this._resolve(fqn, this.aliasLazy, this.aliasCache, 'type alias');
    }
    /**
     * Reverse lookup for the encode path: given a value's constructor, return
     * its BAML FQN, or "" if it is not a codegen-emitted class. Builds the
     * reverse map lazily by resolving every class/enum thunk once.
     */
    jsTypeToBamlType(ctor) {
        if (this.reverse === null) {
            this.reverse = new Map();
            for (const [fqn, thunk] of this.classLazy) {
                try {
                    this.reverse.set(thunk(), fqn);
                }
                catch {
                    /* unresolvable entry — skip */
                }
            }
            for (const [fqn, thunk] of this.enumLazy) {
                try {
                    this.reverse.set(thunk(), fqn);
                }
                catch {
                    /* unresolvable entry — skip */
                }
            }
        }
        let cur = ctor;
        while (cur != null) {
            const fqn = this.reverse.get(cur);
            if (fqn !== undefined)
                return fqn;
            cur = Object.getPrototypeOf(cur);
        }
        return '';
    }
    warm() {
        for (const k of this.classLazy.keys())
            this.getClass(k);
        for (const k of this.enumLazy.keys())
            this.getEnum(k);
        for (const k of this.aliasLazy.keys())
            this.getTypeAlias(k);
    }
}
let _TYPE_MAP = new BamlTypeMap();
let activeTypeMap;
export function setTypeMap(m, runtime) {
    m.runtime = runtime;
    if (!runtime)
        _TYPE_MAP = m;
}
export function getTypeMap() { return activeTypeMap ?? _TYPE_MAP; }
/** Only synchronous encode/decode sections use ambient context. Never hold it across await. */
export function withTypeMap(m, fn) {
    const previous = activeTypeMap;
    activeTypeMap = m;
    try {
        return fn();
    }
    finally {
        activeTypeMap = previous;
    }
}
export function getRuntime() { return getTypeMap().runtime ?? nativeGetRuntime(); }
/** Bind a runtime without changing the SDK's shared nominal type definitions. */
export function typeMapForRuntime(runtime) {
    const base = getTypeMap();
    if (base.runtime?.runtimeKey === runtime.runtimeKey)
        return base;
    const bound = Object.create(base);
    bound.runtime = runtime;
    return bound;
}
//# sourceMappingURL=typemap.js.map