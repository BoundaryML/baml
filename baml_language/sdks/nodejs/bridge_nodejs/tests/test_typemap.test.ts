// test_typemap.test.ts — coverage for the runtime BamlTypeMap and
// setTypeMap/getTypeMap.

import path from 'node:path';

import {
    BamlTypeMap,
    setTypeMap,
    getTypeMap,
    BamlError,
} from '../dist/index.js';

describe('BamlTypeMap', () => {
    test('empty map getClass throws BamlError', () => {
        const m = new BamlTypeMap();
        expect(() => m.getClass('foo.Bar')).toThrow(BamlError);
    });

    test('lazy thunk resolves and memoizes', () => {
        // A thunk closing over an imported builtin resolves regardless of cwd.
        const m = BamlTypeMap.fromLazyEntries({
            classes: { 'user.PathSep': () => path.sep },
            enums: {},
            typeAliases: {},
        });
        const sep = m.getClass('user.PathSep');
        expect(sep).toBe(path.sep);
        // Second lookup hits the cache (same value).
        expect(m.getClass('user.PathSep')).toBe(sep);
    });

    test('thunk returning undefined throws BamlError', () => {
        const m = BamlTypeMap.fromLazyEntries({
            classes: { 'user.Missing': () => (path as Record<string, unknown>).definitelyNotAnExport },
            enums: {},
            typeAliases: {},
        });
        expect(() => m.getClass('user.Missing')).toThrow(BamlError);
    });

    test('setTypeMap / getTypeMap round-trip', () => {
        const m = new BamlTypeMap();
        setTypeMap(m);
        expect(getTypeMap()).toBe(m);
    });
});
