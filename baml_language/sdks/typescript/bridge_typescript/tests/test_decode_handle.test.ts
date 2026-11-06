// test_decode_handle.test.ts — mirrors bridge_python/tests/test_decode_handle.py.
// Exercises the handle-table free functions added in Phase 1.1.

import { BamlHandle, _seedGenericMediaHandle } from '../dist/index.js';
// The handle-table probes are deliberately NOT on the package root: the bridge
// surface is contracted to be identical in node, browsers and workers, and the
// web bridge has no wasm binding for them. They stay reachable here through the
// native module, the same way the web bridge keeps `_testHandleTableEntryCount`
// off its own root.
import {
    _encodeInbound,
    _handleRefcount,
    _hostValueCount,
    _hostValueRegistered,
    _liveHandleCount,
    _seedHeapHandle,
    releaseHostValueKey,
} from '../dist/native.js';
import { lookupHostValue, registerHostOpaque } from '../dist/host_value_registry.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

const HOST_VALUE_OPAQUE = baml_bridge.cffi.v1.BamlHandleType.HOST_VALUE_OPAQUE;
const hostKeyOf = (key: { low: number; high: number }) => (BigInt(key.high >>> 0) << 32n) | BigInt(key.low >>> 0);

describe('handle table dispatch', () => {
    test('generic media handle wraps from raw key', () => {
        const [key, ht] = _seedGenericMediaHandle();
        const h = new BamlHandle(key, ht);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(h.handleType).toBe(ht);
    });

    test('_cloneKeyForWire returns a fresh key for an existing handle', () => {
        const [key, ht] = _seedGenericMediaHandle();
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

describe('host-value references', () => {
    test('a host value sent again takes a reference, and is refused once every one is given back', async () => {
        const before = _hostValueCount();
        const value = { name: 'sent twice' };
        const key = registerHostOpaque(value);
        const handle = new BamlHandle(key, HOST_VALUE_OPAQUE);
        expect(handle._cloneKeyForWire()).toEqual(key);
        // A wrapper of a host value owns none of its references.
        expect(handle.clone().key).toEqual(key);
        expect(_hostValueCount()).toBe(before + 1);

        releaseHostValueKey(key);
        expect(_hostValueRegistered(key)).toBe(true);
        expect(lookupHostValue(hostKeyOf(key))).toBe(value);
        releaseHostValueKey(key);
        expect(_hostValueRegistered(key)).toBe(false);
        expect(_hostValueCount()).toBe(before);
        expect(() => handle._cloneKeyForWire()).toThrow(/no longer registered/);
        // The value's entry goes with the key's last reference, on the loop.
        await vi.waitFor(() => expect(lookupHostValue(hostKeyOf(key))).toBeUndefined());
    });

    test('a send that never goes out gives back only the reference it took', () => {
        const key = registerHostOpaque('rolled back');
        const handle = new BamlHandle(key, HOST_VALUE_OPAQUE);
        expect(() => _encodeInbound([handle, Symbol('cannot encode')])).toThrow();
        // The registration's own reference is untouched.
        expect(_hostValueRegistered(key)).toBe(true);
        // A send that does go out keeps its reference.
        _encodeInbound(handle);
        releaseHostValueKey(key);
        expect(_hostValueRegistered(key)).toBe(true);
        releaseHostValueKey(key);
        expect(_hostValueRegistered(key)).toBe(false);
    });
});
