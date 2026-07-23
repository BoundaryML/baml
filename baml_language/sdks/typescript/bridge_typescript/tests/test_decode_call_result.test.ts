// test_decode_call_result.test.ts — exercises the BamlOutboundResult decode
// path directly, by hand-building wire holders. Covers two behaviors that the
// engine-driven round-trip tests don't reach:
//   1. inline media_value / prompt_ast_value are rejected (not silently null'd).
//   2. error/panic arms surface a BamlError/BamlPanic carrying the decoded
//      value, BAML trace, and class FQN — not just a formatted string.

import { baml_bridge } from '../dist/proto/baml_cffi.js';
import { decodeCallResult } from '../dist/index.js';
import { BamlClientError, BamlError, BamlInvalidArgumentError, BamlPanic } from '../dist/index.js';

const { BamlOutboundResult, BamlOutboundValue } = baml_bridge.cffi.v1;

function encodeResult(result: baml_bridge.cffi.v1.IBamlOutboundResult): Buffer {
    return Buffer.from(BamlOutboundResult.encode(BamlOutboundResult.create(result)).finish());
}

describe('decodeCallResult — inline media / prompt AST rejection', () => {
    test('an ok envelope carrying media_value is rejected, not collapsed to null', () => {
        const bytes = encodeResult({ ok: { mediaValue: { mimeType: 'image/png' } } });
        expect(() => decodeCallResult(bytes)).toThrow(/media_value on the FFI path/);
    });

    test('an ok envelope carrying prompt_ast_value is rejected', () => {
        const bytes = encodeResult({ ok: { promptAstValue: { simple: {} } } });
        expect(() => decodeCallResult(bytes)).toThrow(/prompt_ast_value on the FFI path/);
    });

    test('a genuinely empty ok envelope still decodes to null', () => {
        expect(decodeCallResult(encodeResult({ ok: {} }))).toBeNull();
    });
});

describe('decodeCallResult — thrown value carries structured detail', () => {
    // An unmapped class FQN decodes to a plain object (the bare bridge has no
    // generated typemap), so `.value` is `{ message }` and `.className` is the FQN.
    const thrown: baml_bridge.cffi.v1.IBamlOutboundValue = {
        classValue: {
            name: 'baml.errors.GenericSdkError',
            fields: [{ key: 'message', value: { stringValue: 'boom' } }],
        },
    };
    const trace = ['File "a.baml", line 3, in foo', 'File "b.baml", line 9, in bar'];

    test('error arm: BamlError carries value, bamlTrace, className', () => {
        const bytes = encodeResult({ error: { value: thrown, trace } });
        let caught: unknown;
        try {
            decodeCallResult(bytes);
        } catch (e) {
            caught = e;
        }
        expect(caught).toBeInstanceOf(BamlClientError);
        const err = caught as BamlError;
        expect(err.className).toBe('baml.errors.GenericSdkError');
        expect(err.bamlTrace).toEqual(trace);
        expect(err.value).toMatchObject({ message: 'boom' });
        // The formatted message still surfaces the className + message + trace.
        expect(err.message).toContain('baml.errors.GenericSdkError');
        expect(err.message).toContain('boom');
    });

    test('documented invalid-argument class selects its exact subclass', () => {
        const invalid = {
            classValue: {
                name: 'baml.errors.InvalidArgument',
                fields: [{ key: 'message', value: { stringValue: 'bad argument' } }],
            },
        };
        expect(() => decodeCallResult(encodeResult({ error: { value: invalid, trace } }))).toThrow(BamlInvalidArgumentError);
    });

    test('panic arm: BamlPanic carries the same structured detail', () => {
        const bytes = encodeResult({ panic: { value: thrown, trace } });
        let caught: unknown;
        try {
            decodeCallResult(bytes);
        } catch (e) {
            caught = e;
        }
        expect(caught).toBeInstanceOf(BamlPanic);
        const panic = caught as BamlPanic;
        expect(panic.className).toBe('baml.errors.GenericSdkError');
        expect(panic.bamlTrace).toEqual(trace);
        expect(panic.value).toMatchObject({ message: 'boom' });
    });
});
