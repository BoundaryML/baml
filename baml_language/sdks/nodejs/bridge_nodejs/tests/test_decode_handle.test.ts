// test_decode_handle.test.ts — mirrors bridge_python/tests/test_decode_handle.py.

import {
    BamlHandle,
    _seedFunctionRefHandle,
    _seedGenericMediaHandle,
    takeHandleFromTable,
    putHandleIntoTable,
} from '../index';

describe('handle table dispatch', () => {
    test('function ref handle round-trips through takeHandleFromTable', () => {
        const seeded = _seedFunctionRefHandle(123);
        const h = takeHandleFromTable(seeded.key, seeded.handleType);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(h.handleType).toBe(seeded.handleType);
    });

    test('generic media handle round-trips through takeHandleFromTable', () => {
        const seeded = _seedGenericMediaHandle();
        const h = takeHandleFromTable(seeded.key, seeded.handleType);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(h.handleType).toBe(seeded.handleType);
    });

    test('takeHandleFromTable rejects an unknown key', () => {
        expect(() =>
            takeHandleFromTable({ low: -1, high: -1 }, 0),
        ).toThrow(/not in HANDLE_TABLE/);
    });

    test('putHandleIntoTable clones the row and keeps the source usable', () => {
        const seeded = _seedFunctionRefHandle(7);
        const h = takeHandleFromTable(seeded.key, seeded.handleType);
        const cloned = putHandleIntoTable(h);
        expect(cloned.handleType).toBe(seeded.handleType);
        // Source handle still resolves to a value via clone()
        const cloneViaMethod = h.clone();
        expect(cloneViaMethod).toBeInstanceOf(BamlHandle);
    });
});
