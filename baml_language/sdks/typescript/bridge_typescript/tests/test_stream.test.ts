// test_stream.test.ts — Phase 1 unit coverage for the BamlStream wrapper.
//
// End-to-end next()/final() needs a streaming BAML fixture and is deferred to
// Phase 4. Here we validate the wrapper class structure and that a stream-typed
// value encodes without throwing (the _toHandle round-trip through the encoder).

import { BamlStream, BamlHandle, _seedFunctionRefHandle, encodeCallArgs } from '../dist/index.js';

describe('BamlStream', () => {
    test('_toHandle round-trips through encodeCallArgs', () => {
        // Seed any handle (a function ref is fine for the encoder test).
        const [key, ht] = _seedFunctionRefHandle(42);
        const h = new BamlHandle(key, ht);
        const stream = BamlStream._fromHandle<number, string>(h, 'ai.stream.Stream');

        const innerH = stream._toHandle();
        expect(innerH).toBeInstanceOf(BamlHandle);
        expect((stream as unknown as { _classFqn: string })._classFqn).toBe('ai.stream.Stream');

        // Encoding a stream-typed value should not throw.
        expect(() => encodeCallArgs({ self: stream }, { callId: 1n })).not.toThrow();
    });
});
