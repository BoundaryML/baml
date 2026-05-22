// typemap.ts — runtime BamlTypeMap analog of bridge_python/baml_core/typemap.py.
//
// Codegen emits a `_typemap.ts` per SDK that calls
// `BamlTypeMap.fromLazyEntries({ classes, enums, typeAliases })`.
// The root `index.ts` of the generated SDK then calls
// `setTypeMap(_TYPE_MAP)`. Decoders look up FQNs via `getClass`/`getEnum`
// to materialize wire values into the host classes the codegen emitted.

import { BamlError } from './errors';

export type LazyEntry = [string, string]; // [modulePath, attrName]

// Hard-coded stdlib reverse-overrides. Phase 4 will populate this map
// with the native class identities (BamlImage etc.) so that user code
// passing those classes through generic positions can be reflected back
// to the right BAML FQN. Phase 2 leaves it empty.
const _STDLIB_REVERSE_OVERRIDES: Map<string, string> = new Map();

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

    getClass(fqn: string): unknown {
        return this._resolve(fqn, this.classLazy, this.classCache, 'class');
    }
    getEnum(fqn: string): unknown {
        return this._resolve(fqn, this.enumLazy, this.enumCache, 'enum');
    }
    getTypeAlias(fqn: string): unknown {
        return this._resolve(fqn, this.aliasLazy, this.aliasCache, 'type alias');
    }

    private _resolve(
        fqn: string,
        lazy: Map<string, LazyEntry>,
        cache: Map<string, unknown>,
        kind: string,
    ): unknown {
        const cached = cache.get(fqn);
        if (cached !== undefined) return cached;
        const entry = lazy.get(fqn);
        if (entry === undefined) {
            throw new BamlError(`Unknown ${kind} FQN ${fqn}`);
        }
        const [modulePath, attr] = entry;
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        const mod: Record<string, unknown> = require(modulePath);
        const obj = mod[attr];
        if (obj === undefined) {
            throw new BamlError(
                `Could not resolve ${kind} ${fqn} → ${modulePath}.${attr}`,
            );
        }
        cache.set(fqn, obj);
        return obj;
    }

    /// Reverse-lookup: given a host class identity (`cls.__bamlModulePath` +
    /// `cls.name`), return the BAML FQN. Walks the prototype chain so user
    /// subclasses of codegen-emitted classes still match. Returns `""` if
    /// no entry matches.
    jsTypeToBamlType(cls: unknown): string {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        let cur: any = cls;
        while (cur != null) {
            const name = cur?.name;
            const mod = cur?.__bamlModulePath;
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
