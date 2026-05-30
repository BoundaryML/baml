// test_decode_handle.test.ts — mirrors bridge_python/tests/test_decode_handle.py.
// Exercises the handle-table free functions added in Phase 1.1.

import {
    BamlHandle,
    _seedFunctionRefHandle,
    _seedGenericMediaHandle,
    takeHandleFromTable,
    putHandleIntoTable,
} from '../index';

describe('handle table dispatch', () => {
    test('function ref handle round-trips through takeHandleFromTable', () => {
        const [key, ht] = _seedFunctionRefHandle(123);
        const h = takeHandleFromTable(key, ht);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(h.handleType).toBe(ht);
    });

    test('generic media handle round-trips through takeHandleFromTable', () => {
        const [key, ht] = _seedGenericMediaHandle();
        const h = takeHandleFromTable(key, ht);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(h.handleType).toBe(ht);
    });

    test('putHandleIntoTable returns a fresh key for an existing handle', () => {
        const [key, ht] = _seedFunctionRefHandle(7);
        const h = takeHandleFromTable(key, ht);
        const newKey = putHandleIntoTable(h);
        // The cloned key shares the same Arc; it must still resolve.
        const h2 = takeHandleFromTable(newKey, ht);
        expect(h2).toBeInstanceOf(BamlHandle);
    });

    test('takeHandleFromTable rejects an unknown key', () => {
        expect(() => takeHandleFromTable({ low: 999999, high: 0 }, 5)).toThrow();
    });
});
