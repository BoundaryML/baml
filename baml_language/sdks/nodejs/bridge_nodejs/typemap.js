/**
 * THIS FILE IS AUTO-GENERATED — DO NOT EDIT BY HAND.
 *
 * Source: baml_language/crates/bridge_nodejs/typescript_src/
 * Proto:  baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/*.proto
 * Build:  cd baml_language/crates/bridge_nodejs && pnpm build:debug
 */
"use strict";
// typemap.ts — runtime BamlTypeMap analog of bridge_python/baml_core/typemap.py.
//
// Codegen emits a `_typemap.ts` per SDK that calls
// `BamlTypeMap.fromLazyEntries({ classes, enums, typeAliases })`.
// The root `index.ts` of the generated SDK then calls
// `setTypeMap(_TYPE_MAP)`. Decoders look up FQNs via `getClass`/`getEnum`
// to materialize wire values into the host classes the codegen emitted.
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlTypeMap = void 0;
exports.setTypeMap = setTypeMap;
exports.getTypeMap = getTypeMap;
const errors_1 = require("./errors");
// Hard-coded stdlib reverse-overrides. Phase 4 will populate this map
// with the native class identities (BamlImage etc.) so that user code
// passing those classes through generic positions can be reflected back
// to the right BAML FQN. Phase 2 leaves it empty.
const _STDLIB_REVERSE_OVERRIDES = new Map();
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
    getClass(fqn) {
        return this._resolve(fqn, this.classLazy, this.classCache, 'class');
    }
    getEnum(fqn) {
        return this._resolve(fqn, this.enumLazy, this.enumCache, 'enum');
    }
    getTypeAlias(fqn) {
        return this._resolve(fqn, this.aliasLazy, this.aliasCache, 'type alias');
    }
    _resolve(fqn, lazy, cache, kind) {
        const cached = cache.get(fqn);
        if (cached !== undefined)
            return cached;
        const entry = lazy.get(fqn);
        if (entry === undefined) {
            throw new errors_1.BamlError(`Unknown ${kind} FQN ${fqn}`);
        }
        const [modulePath, attr] = entry;
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        const mod = require(modulePath);
        const obj = mod[attr];
        if (obj === undefined) {
            throw new errors_1.BamlError(`Could not resolve ${kind} ${fqn} → ${modulePath}.${attr}`);
        }
        cache.set(fqn, obj);
        return obj;
    }
    /// Reverse-lookup: given a host class identity (`cls.__bamlModulePath` +
    /// `cls.name`), return the BAML FQN. Walks the prototype chain so user
    /// subclasses of codegen-emitted classes still match. Returns `""` if
    /// no entry matches.
    jsTypeToBamlType(cls) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        let cur = cls;
        while (cur != null) {
            const name = cur?.name;
            const mod = cur?.__bamlModulePath;
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