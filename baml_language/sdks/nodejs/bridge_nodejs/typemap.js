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
// enums, typeAliases })` where each entry is `[modulePath, attrName]`. The
// root `index.ts` calls `setTypeMap(_TYPE_MAP)` at import time. Resolution is
// lazy: `getClass(fqn)` does `require(modulePath)[attrName]` on first lookup
// and memoizes. (The decode-side walk that consumes this lands in Phase 5.)
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlTypeMap = void 0;
exports.setTypeMap = setTypeMap;
exports.getTypeMap = getTypeMap;
const errors_1 = require("./errors");
// Hard-coded stdlib reverse-overrides. Mirrors _STDLIB_REVERSE_OVERRIDES in
// baml_core/typemap.py. Keys are `${modulePath}::${exportName}` of the native
// class identities the codegen-emitted re-exports point at. Phase 4/5 seed the
// real native identities (BamlImage/…/BamlStream); Phase 2 leaves it empty and
// documents the intent.
const _STDLIB_REVERSE_OVERRIDES = new Map([
// ["@boundaryml/baml-core::BamlImage", "baml.media.Image"], — wired in Phase 4/5
]);
class BamlTypeMap {
    constructor() {
        this.classLazy = new Map();
        this.enumLazy = new Map();
        this.aliasLazy = new Map();
        this.classCache = new Map();
        this.enumCache = new Map();
        this.aliasCache = new Map();
        this.reverse = new Map(_STDLIB_REVERSE_OVERRIDES);
    }
    static fromLazyEntries(args) {
        const m = new BamlTypeMap();
        for (const [fqn, le] of Object.entries(args.classes))
            m.classLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.enums))
            m.enumLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.typeAliases))
            m.aliasLazy.set(fqn, le);
        for (const [fqn, [mp, attr]] of Object.entries(args.classes)) {
            const k = `${mp}::${attr}`;
            if (!m.reverse.has(k))
                m.reverse.set(k, fqn);
        }
        for (const [fqn, [mp, attr]] of Object.entries(args.enums)) {
            const k = `${mp}::${attr}`;
            if (!m.reverse.has(k))
                m.reverse.set(k, fqn);
        }
        return m;
    }
    _resolve(fqn, lazy, cache, kind) {
        if (cache.has(fqn))
            return cache.get(fqn);
        const entry = lazy.get(fqn);
        if (entry === undefined)
            throw new errors_1.BamlError(`Unknown ${kind} FQN ${fqn}`);
        const [modulePath, attr] = entry;
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const mod = require(modulePath);
        const resolved = mod[attr];
        if (resolved === undefined) {
            throw new errors_1.BamlError(`Could not resolve ${fqn} → ${modulePath}.${attr}`);
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
    // Walk the prototype chain. Returns "" if no match. Phase 5 refines this
    // for class identity. (Provisional — see 10a-todo-items.md Phase D.)
    jsTypeToBamlType(cls) {
        let cur = cls;
        while (cur != null) {
            const name = cur.name;
            const mod = cur.__bamlModulePath;
            if (mod && name) {
                const fqn = this.reverse.get(`${mod}::${name}`);
                if (fqn !== undefined)
                    return fqn;
            }
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