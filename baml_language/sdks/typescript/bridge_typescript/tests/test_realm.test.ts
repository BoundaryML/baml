// A plain object from another realm (a `node:vm` context) is a plain object:
// its prototype is that realm's `Object.prototype`, not ours. The bridge asks
// the runtime's adapter rather than comparing prototypes itself, so the
// object encodes as a map or class like one made here.
import { runInNewContext } from 'node:vm';
// The runtime package registers the host adapter the bridge encodes through.
import '../dist/index.js';
import { _encodeInbound } from '../dist/native.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

const { InboundValue } = baml_bridge.cffi.v1;

describe('plain objects across realms', () => {
    test('an object from a vm context encodes as a plain object', () => {
        const foreign = runInNewContext('({ a: 1, b: "two" })') as object;
        expect(Object.getPrototypeOf(foreign)).not.toBe(Object.prototype);
        const local = { a: 1, b: 'two' };
        const encode = (value: object) => InboundValue.decode(_encodeInbound(value, 'self')).toJSON();
        expect(encode(foreign)).toEqual(encode(local));
    });

    test('a Uint8Array from a vm context encodes as bytes', () => {
        // `instanceof Uint8Array` against this realm's constructor would miss
        // it; the adapter answers by the view's own brand.
        const foreign = runInNewContext('new Uint8Array([1, 2, 3])') as object;
        expect(foreign instanceof Uint8Array).toBe(false);
        const encode = (value: object) => InboundValue.decode(_encodeInbound(value, 'self')).toJSON();
        expect(encode(foreign)).toEqual(encode(new Uint8Array([1, 2, 3])));
    });

    test('a foreign Map is still not a plain object', () => {
        const foreignMap = runInNewContext('new Map([["a", 1]])') as object;
        expect(() => _encodeInbound(foreignMap, 'self')).toThrow();
    });
});
