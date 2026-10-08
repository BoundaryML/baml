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
import { BamlError } from './errors.js';
import { registerProgramDescriptor } from './native.js';
export class BamlTypeMap {
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
    // The embedded program descriptor (the program's declared surface, as the
    // generator encoded it), or null for a typemap that declares no types.
    // TypeScript never reads it: the bridge decodes it natively and encodes
    // arguments against the types it declares. A typemap without one (bridge
    // tests, call-by-name without a generated SDK) declares no types, and the
    // bridge then takes each argument as the type its JavaScript value has.
    _descriptor = null;
    static fromLazyEntries(args) {
        const m = new BamlTypeMap();
        for (const [fqn, le] of Object.entries(args.classes))
            m.classLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.enums))
            m.enumLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.typeAliases))
            m.aliasLazy.set(fqn, le);
        m._descriptor = args.descriptor ?? null;
        return m;
    }
    /** The embedded program descriptor, or null when this typemap declares no types. */
    get descriptor() {
        return this._descriptor;
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
    ensureReverse() {
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
    }
    /** Identity-only lookup for capture; never inspects application prototypes. */
    captureType(ctor) {
        this.ensureReverse();
        return this.reverse.get(ctor) ?? '';
    }
    jsTypeToBamlType(ctor) {
        this.ensureReverse();
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
/** Install `m` as the runtime typemap and hand its program descriptor to the
 * bridge, which encodes every argument against the types it declares. */
export function setTypeMap(m) {
    _TYPE_MAP = m;
    registerProgramDescriptor(m.descriptor);
}
export function getTypeMap() {
    return _TYPE_MAP;
}
//# sourceMappingURL=typemap.js.map