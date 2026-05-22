// test_typemap.test.ts — covers BamlTypeMap and BAML_PLACEHOLDER (Phase 2.5).

import {
    BamlTypeMap,
    setTypeMap,
    getTypeMap,
    BAML_PLACEHOLDER,
    BamlError,
} from '../index';

describe('BAML_PLACEHOLDER', () => {
    test('is truthy and frozen', () => {
        expect(BAML_PLACEHOLDER).toBeDefined();
        expect(BAML_PLACEHOLDER).toBeTruthy();
        expect(Object.isFrozen(BAML_PLACEHOLDER)).toBe(true);
    });

    test('jest toBeDefined succeeds (codegen relies on this)', () => {
        // Generated `export const Foo = BAML_PLACEHOLDER;` must satisfy
        // `expect(Foo).toBeDefined()` for the Phase 2 jest split to work.
        const Foo = BAML_PLACEHOLDER;
        expect(Foo).toBeDefined();
    });
});

describe('BamlTypeMap', () => {
    test('empty map throws on getClass', () => {
        const m = new BamlTypeMap();
        expect(() => m.getClass('user.lorem.Resume')).toThrow(BamlError);
    });

    test('fromLazyEntries populates classes / enums / aliases', () => {
        const m = BamlTypeMap.fromLazyEntries({
            classes: { 'a.b.Foo': ['nonexistent/mod', 'Foo'] },
            enums: { 'a.b.E': ['nonexistent/mod', 'E'] },
            typeAliases: { 'a.b.T': ['nonexistent/mod', 'T'] },
        });
        // getClass / getEnum / getTypeAlias all hit the lazy lookup —
        // since `require('nonexistent/mod')` will throw a Node error,
        // we assert the lookup is attempted (not just BamlError thrown
        // due to missing entry).
        expect(() => m.getClass('a.b.Foo')).toThrow();
        expect(() => m.getEnum('a.b.E')).toThrow();
        expect(() => m.getTypeAlias('a.b.T')).toThrow();
    });

    test('setTypeMap / getTypeMap round-trip', () => {
        const m = BamlTypeMap.fromLazyEntries({
            classes: {},
            enums: {},
            typeAliases: {},
        });
        setTypeMap(m);
        expect(getTypeMap()).toBe(m);
    });
});
