// typemap.ts — runtime BamlTypeMap, the Node analog of
// sdks/python/src/baml_core/typemap.py.
//
// Codegen emits `_typemap.ts` with `BamlTypeMap.fromLazyEntries({ classes,
// enums, typeAliases })` where each entry is `[modulePath, attrName]`. The
// root `index.ts` calls `setTypeMap(_TYPE_MAP)` at import time. Resolution is
// lazy: `getClass(fqn)` does `require(modulePath)[attrName]` on first lookup
// and memoizes. (The decode-side walk that consumes this lands in Phase 5.)

import { BamlError } from './errors';

export type LazyEntry = [string, string]; // [modulePath, attrName]

// Hard-coded stdlib reverse-overrides. Mirrors _STDLIB_REVERSE_OVERRIDES in
// baml_core/typemap.py. Keys are `${modulePath}::${exportName}` of the native
// class identities the codegen-emitted re-exports point at. Phase 4/5 seed the
// real native identities (BamlImage/…/BamlStream); Phase 2 leaves it empty and
// documents the intent.
const _STDLIB_REVERSE_OVERRIDES: Map<string, string> = new Map([
    // ["@boundaryml/baml-core::BamlImage", "baml.media.Image"], — wired in Phase 4/5
]);

export class BamlTypeMap {
    private classLazy = new Map<string, LazyEntry>();
    private enumLazy = new Map<string, LazyEntry>();
    private aliasLazy = new Map<string, LazyEntry>();
    private classCache = new Map<string, unknown>();
    private enumCache = new Map<string, unknown>();
    private aliasCache = new Map<string, unknown>();
    private reverse: Map<string, string> = new Map(_STDLIB_REVERSE_OVERRIDES);

    static fromLazyEntries(args: {
        classes: Record<string, LazyEntry>;
        enums: Record<string, LazyEntry>;
        typeAliases: Record<string, LazyEntry>;
    }): BamlTypeMap {
        const m = new BamlTypeMap();
        for (const [fqn, le] of Object.entries(args.classes)) m.classLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.enums)) m.enumLazy.set(fqn, le);
        for (const [fqn, le] of Object.entries(args.typeAliases)) m.aliasLazy.set(fqn, le);
        for (const [fqn, [mp, attr]] of Object.entries(args.classes)) {
            const k = `${mp}::${attr}`;
            if (!m.reverse.has(k)) m.reverse.set(k, fqn);
        }
        for (const [fqn, [mp, attr]] of Object.entries(args.enums)) {
            const k = `${mp}::${attr}`;
            if (!m.reverse.has(k)) m.reverse.set(k, fqn);
        }
        return m;
    }

    private _resolve(
        fqn: string,
        lazy: Map<string, LazyEntry>,
        cache: Map<string, unknown>,
        kind: string,
    ): unknown {
        if (cache.has(fqn)) return cache.get(fqn);
        const entry = lazy.get(fqn);
        if (entry === undefined) throw new BamlError(`Unknown ${kind} FQN ${fqn}`);
        const [modulePath, attr] = entry;
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const mod = require(modulePath);
        const resolved = mod[attr];
        if (resolved === undefined) {
            throw new BamlError(`Could not resolve ${fqn} → ${modulePath}.${attr}`);
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

    // Walk the prototype chain. Returns "" if no match. Phase 5 refines this
    // for class identity. (Provisional — see 10a-todo-items.md Phase D.)
    jsTypeToBamlType(cls: unknown): string {
        let cur: unknown = cls;
        while (cur != null) {
            const name = (cur as { name?: string }).name;
            const mod = (cur as { __bamlModulePath?: string }).__bamlModulePath;
            if (mod && name) {
                const fqn = this.reverse.get(`${mod}::${name}`);
                if (fqn !== undefined) return fqn;
            }
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
export function setTypeMap(m: BamlTypeMap): void {
    _TYPE_MAP = m;
}
export function getTypeMap(): BamlTypeMap {
    return _TYPE_MAP;
}
