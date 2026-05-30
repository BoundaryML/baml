// test_typemap.test.ts — Phase 2.5 coverage for the runtime BamlTypeMap,
// setTypeMap/getTypeMap, and the BAML_PLACEHOLDER sentinel.

import {
    BamlTypeMap,
    setTypeMap,
    getTypeMap,
    BAML_PLACEHOLDER,
    BamlError,
} from '../index';

describe('BamlTypeMap', () => {
    test('empty map getClass throws BamlError', () => {
        const m = new BamlTypeMap();
        expect(() => m.getClass('foo.Bar')).toThrow(BamlError);
    });

    test('lazy entry resolves via require and memoizes', () => {
        // Point at a node builtin so require resolves regardless of cwd:
        // require("path").sep is always defined.
        const m = BamlTypeMap.fromLazyEntries({
            classes: { 'user.PathSep': ['path', 'sep'] },
            enums: {},
            typeAliases: {},
        });
        const sep = m.getClass('user.PathSep');
        expect(sep).toBe(require('path').sep);
        // Second lookup hits the cache (same value).
        expect(m.getClass('user.PathSep')).toBe(sep);
    });

    test('unresolvable attr throws BamlError', () => {
        const m = BamlTypeMap.fromLazyEntries({
            classes: { 'user.Missing': ['path', 'definitelyNotAnExport'] },
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

describe('BAML_PLACEHOLDER', () => {
    test('is a frozen, defined sentinel', () => {
        expect(BAML_PLACEHOLDER).toBeDefined();
        expect(Object.isFrozen(BAML_PLACEHOLDER)).toBe(true);
    });
});
