/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// typemap.ts — runtime BamlTypeMap, the Node analog of
// sdks/python/src/baml_core/typemap.py.
//
// Codegen emits `_typemap.ts` with `BamlTypeMap.fromLazyEntries({ classes,
// enums, typeAliases })` where each entry is a RESOLVER THUNK
// `() => require("./lorem").Resume`. The thunk closes over a `require`
// relative to the generated `_typemap.ts`, so resolution happens in the SDK's
// module scope (the runtime package can't resolve a `baml_sdk/...` path). The
// root `index.ts` calls `setTypeMap(_TYPE_MAP)` at import time; resolution is
// lazy and memoized on first lookup, which also avoids the circular
// `index ↔ _typemap` import deadlocking.
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlTypeMap = void 0;
exports.setTypeMap = setTypeMap;
exports.getTypeMap = getTypeMap;
const errors_1 = require("./errors");
class BamlTypeMap {
    constructor() {
        this.classLazy = new Map();
        this.enumLazy = new Map();
        this.aliasLazy = new Map();
        this.classCache = new Map();
        this.enumCache = new Map();
        this.aliasCache = new Map();
        // Reverse map (constructor identity → FQN) for the encode path. Lazily
        // built from the class/enum thunks on first `jsTypeToBamlType` call. The
        // five stdlib media/stream wrappers encode via `instanceof` in proto.ts,
        // so they don't need to be seeded here.
        this.reverse = null;
    }
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
            throw new errors_1.BamlError(`Unknown ${kind} FQN ${fqn}`);
        let resolved;
        try {
            resolved = thunk();
        }
        catch (e) {
            throw new errors_1.BamlError(`Failed to resolve ${kind} ${fqn}: ${String(e)}`);
        }
        if (resolved === undefined) {
            throw new errors_1.BamlError(`Could not resolve ${kind} ${fqn} (resolver returned undefined)`);
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
exports.BamlTypeMap = BamlTypeMap;
let _TYPE_MAP = new BamlTypeMap();
function setTypeMap(m) {
    _TYPE_MAP = m;
}
function getTypeMap() {
    return _TYPE_MAP;
}
//# sourceMappingURL=typemap.js.map