// test_stream.test.ts — unit tests for BamlStream wrapper shape.
//
// End-to-end next/final exercising baml.llm.Stream.* via the engine is
// Phase 4 territory; this file only validates that the TS wrapper class
// is shaped right and round-trips through encodeCallArgs as a handle.

import {
    BamlStream,
    BamlHandle,
    _seedFunctionRefHandle,
    encodeCallArgs,
    takeHandleFromTable,
} from '../index';

describe('BamlStream', () => {
    test('_fromHandle / _toHandle round-trip', () => {
        const seeded = _seedFunctionRefHandle(42);
        const h = takeHandleFromTable(seeded.key, seeded.handleType);
        const stream = BamlStream._fromHandle<number, string>(h);
        const inner = stream._toHandle();
        expect(inner).toBeInstanceOf(BamlHandle);
        expect(inner.handleType).toBe(seeded.handleType);
    });

    test('encodeCallArgs accepts a BamlStream-typed kwarg', () => {
        const seeded = _seedFunctionRefHandle(99);
        const h = takeHandleFromTable(seeded.key, seeded.handleType);
        const stream = BamlStream._fromHandle<number, string>(h);
        expect(() => encodeCallArgs({ self: stream })).not.toThrow();
    });
});
