// test_decode_handle.test.ts — mirrors bridge_python/tests/test_decode_handle.py.
// Exercises the handle-table free functions added in Phase 1.1.

import { BamlHandle, _seedFunctionRefHandle, _seedGenericMediaHandle } from '../dist/index.js';
// The handle-table probes are deliberately NOT on the package root: the bridge
// surface is contracted to be identical in node, browsers and workers, and the
// web bridge has no wasm binding for them. They stay reachable here through the
// native module, the same way the web bridge keeps `_testHandleTableEntryCount`
// off its own root.
import { _handleRefcount, _liveHandleCount, _seedHeapHandle } from '../dist/native.js';

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

    test('engine-heap handles dedup to one refcounted key', () => {
        // The identity-bearing arm: one heap object owns ONE table key however
        // many times it crosses, and every crossing is one more ownership of
        // that key — rows do not grow, refcounts do.
        const before = _liveHandleCount();
        const [key1, ht] = _seedHeapHandle(7001);
        const [key2] = _seedHeapHandle(7001);
        expect(key2).toEqual(key1);
        expect(_liveHandleCount()).toBe(before + 1);
        // Two crossings, two owed releases: the row count hides that, the
        // refcount shows it.
        expect(_handleRefcount(key1)).toBe(2);

        // Cloning for the wire keeps the key (a copy is another owner of the
        // same identity), unlike the fresh key an identity-free row gets.
        const h = new BamlHandle(key1, ht);
        expect(h._cloneKeyForWire()).toEqual(key1);
        expect(_liveHandleCount()).toBe(before + 1);
        expect(_handleRefcount(key1)).toBe(3);
        expect(_handleRefcount({ low: 999999, high: 0 })).toBeNull();

        // A different object is a different key.
        const [key3] = _seedHeapHandle(7002);
        expect(key3).not.toEqual(key1);
        expect(_liveHandleCount()).toBe(before + 2);
    });

    test('unknown keys fail when used', () => {
        const h = new BamlHandle({ low: 999999, high: 0 }, 5);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(() => h.clone()).toThrow(/invalid handle/);
    });
});
