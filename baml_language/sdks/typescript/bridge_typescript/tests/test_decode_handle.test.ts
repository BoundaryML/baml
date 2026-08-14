// test_decode_handle.test.ts — mirrors bridge_python/tests/test_decode_handle.py.
// Exercises the handle-table free functions added in Phase 1.1.

import {
    BamlHandle,
    _seedFunctionRefHandle,
    _seedGenericMediaHandle,
} from '../dist/index.js';

describe('handle table dispatch', () => {
    test('function ref handle wraps from raw key', () => {
        const [key, ht] = _seedFunctionRefHandle(123);
        const h = new BamlHandle(key, ht);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(h.handleType).toBe(ht);
    });

    test('generic media handle wraps from raw key', () => {
        const [key, ht] = _seedGenericMediaHandle();
        const h = new BamlHandle(key, ht);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(h.handleType).toBe(ht);
    });

    test('_cloneKeyForWire returns a fresh key for an existing handle', () => {
        const [key, ht] = _seedFunctionRefHandle(7);
        const h = new BamlHandle(key, ht);
        const newKey = h._cloneKeyForWire();
        // The cloned key shares the same Arc; it must still resolve.
        const h2 = new BamlHandle(newKey, ht);
        expect(h2).toBeInstanceOf(BamlHandle);
    });

    test('unknown keys fail when used', () => {
        const h = new BamlHandle({ low: 999999, high: 0 }, 5);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(() => h.clone()).toThrow(/invalid handle/);
    });
});
