// test_stream.test.ts — Phase 1 unit coverage for the BamlStream wrapper.
//
// End-to-end next()/final() needs a streaming BAML fixture and is deferred to
// Phase 4. Here we validate the wrapper class structure and that a stream-typed
// value encodes without throwing (the _toHandle round-trip through the bridge).

import { BamlStream, BamlHandle, _seedGenericMediaHandle } from '../dist/index.js';
import { _encodeInbound } from '../dist/native.js';

describe('BamlStream', () => {
    test('_toHandle round-trips through the bridge encoder', () => {
        // Seed any handle (a media row is fine for the encoder test).
        const [key, ht] = _seedGenericMediaHandle();
        const h = new BamlHandle(key, ht);
        const stream = BamlStream._fromHandle<number, string>(h, 'ai.stream.Stream');

        const innerH = stream._toHandle();
        expect(innerH).toBeInstanceOf(BamlHandle);
        expect((stream as unknown as { _classFqn: string })._classFqn).toBe('ai.stream.Stream');

        // Encoding a stream-typed value should not throw.
        expect(() => _encodeInbound(stream, 'self')).not.toThrow();
    });
});
