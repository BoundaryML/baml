// test_decode_call_result.test.ts — exercises the BamlOutboundResult decode
// path directly, by hand-building wire holders. Covers two behaviors that the
// engine-driven round-trip tests don't reach:
//   1. inline media_value / prompt_ast_value reconstruct portable wrappers.
//   2. error/panic arms surface a BamlError/BamlPanic carrying the decoded
//      value, BAML trace, and class FQN — not just a formatted string.

import { baml_bridge } from '../dist/proto/baml_cffi.js';
import { decodeCallResult } from '../dist/index.js';
import {
    BamlClientError,
    BamlError,
    BamlFunctionSpec,
    BamlInvalidArgumentError,
    BamlPanic,
    BamlPrompt,
    _seedGenericMediaHandle,
} from '../dist/index.js';

import { _handleRefcount, releaseHostValueKey } from '../dist/native.js';
import { registerHostOpaque } from '../dist/host_value_registry.js';

const { BamlOutboundResult, BamlOutboundValue, BamlHandleType } = baml_bridge.cffi.v1;

function encodeResult(result: baml_bridge.cffi.v1.IBamlOutboundResult): Buffer {
    return Buffer.from(BamlOutboundResult.encode(BamlOutboundResult.create(result)).finish());
}

describe('decodeCallResult — portable prompt AST', () => {
    test('an ok envelope carrying prompt_ast_value returns a portable wrapper', () => {
        const bytes = encodeResult({ ok: { promptAstValue: { simple: { string: 'hello' } } } });
        const prompt = decodeCallResult(bytes);
        expect(prompt).toBeInstanceOf(BamlPrompt);
        expect((prompt as BamlPrompt).toJSON()).toMatchObject({
            simple: { string: 'hello' },
        });
    });

    test('a genuinely empty ok envelope still decodes to null', () => {
        expect(decodeCallResult(encodeResult({ ok: {} }))).toBeNull();
    });
});

describe('decodeCallResult — FunctionSpec capability', () => {
    test('the dedicated handle tag decodes to BamlFunctionSpec', () => {
        const [key] = _seedGenericMediaHandle();
        const spec = decodeCallResult(encodeResult({
            ok: { handleValue: { key, handleType: 17 } },
        }));

        expect(spec).toBeInstanceOf(BamlFunctionSpec);
        expect((spec as BamlFunctionSpec<unknown>)._toHandle().handleType).toBe(17);
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

describe('decodeCallResult — pinned host values', () => {
    test('a host value in a result releases its pin once read', () => {
        const [pin] = _seedGenericMediaHandle();
        const key = { low: 0x7fff_0002, high: 0 };
        const decoded = decodeCallResult(encodeResult({
            ok: { handleValue: { key, handleType: BamlHandleType.HOST_VALUE_CALLABLE, pin } },
        }));
        expect((decoded as { key: unknown }).key).toEqual(key);
        expect(_handleRefcount(pin)).toBeNull();
    });

    test('a host error rehydrates while its pin is held, and the pin is released', () => {
        const raised = new Error('pinned');
        const key = registerHostOpaque(raised);
        const [pin] = _seedGenericMediaHandle();
        try {
            const bytes = encodeResult({
                error: {
                    value: {
                        classValue: {
                            name: 'baml.errors.HostCallable',
                            fields: [
                                { key: 'message', value: { stringValue: 'pinned' } },
                                { key: '_handle', value: { handleValue: { key, handleType: BamlHandleType.HOST_VALUE_OPAQUE, pin } } },
                            ],
                        },
                    },
                },
            });
            let caught: unknown;
            try { decodeCallResult(bytes); } catch (error) { caught = error; }
            expect(caught).toBe(raised);
            expect(_handleRefcount(pin)).toBeNull();
        } finally {
            releaseHostValueKey(key);
        }
    });
});
