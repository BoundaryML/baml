// The walker reads a container's children one at a time under one budget per
// call; a host-reported `length` sizes nothing. A sparse array of any length
// is therefore refused at the budget after reading that many holes, never
// materialized into millions of handles first.
// The runtime package registers the host adapter the bridge encodes through.
import '../dist/index.js';
import { _encodeInbound } from '../dist/native.js';

describe('reader bounds', () => {
    test('a sparse array far beyond the node budget is refused at the budget, not materialized', () => {
        const huge = new Array(2 ** 24); // 16,777,216 holes; the budget is 1,048,576 nodes
        const started = Date.now();
        expect(() => _encodeInbound(huge, 'self')).toThrow(/more than 1048576 nodes/);
        // Reading a million holes is cheap; allocating sixteen million is not.
        expect(Date.now() - started).toBeLessThan(10_000);
    });

    test('an array whose length lies through a Proxy is read by what it yields', () => {
        // `length` says a billion; the elements end after three.
        const lying = new Proxy([1, 2, 3], {
            get(target, property, receiver) {
                if (property === 'length') return 1_000_000_000;
                return Reflect.get(target, property, receiver);
            },
        });
        expect(() => _encodeInbound(lying, 'self')).toThrow(/more than 1048576 nodes/);
    });
});
