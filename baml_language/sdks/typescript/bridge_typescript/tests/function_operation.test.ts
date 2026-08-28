import { describe, expect, it } from 'vitest';

import { BamlPrompt, decodeCallResult, encodeCallArgs } from '../dist/index.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

const CallFunctionArgs = baml_bridge.cffi.v1.CallFunctionArgs;

describe('FunctionOperation wire encoding', () => {
    it.each([
        ['direct', 0],
        ['spec', 1],
        ['stream', 2],
    ] as const)('encodes %s against the authored FQN', (operation, expected) => {
        const encoded = encodeCallArgs({}, {
            callId: 7n,
            functionName: 'user.Extract',
            operation,
        });
        const call = CallFunctionArgs.decode(encoded);
        expect(call.functionName).toBe('user.Extract');
        expect(call.operation).toBe(expected);
        expect(call.functionName).not.toContain('$');
    });

    it('rejects a non-direct operation without an authored function target', () => {
        expect(() => encodeCallArgs({}, { callId: 7n, operation: 'spec' })).toThrow(
            /requires a function target/,
        );
    });

    it('carries the operation for first-class function handles too', () => {
        const encoded = encodeCallArgs({}, {
            callId: 9n,
            functionHandle: 41n,
            operation: 'spec',
        });
        const call = CallFunctionArgs.decode(encoded);
        expect(call.functionHandle?.toString()).toBe('41');
        expect(call.operation).toBe(1);
    });
});

describe('portable Prompt transport', () => {
    it('decodes and re-encodes the canonical prompt tree without a handle', () => {
        const outbound = baml_bridge.cffi.v1.BamlOutboundResult.encode({
            ok: {
                promptAstValue: {
                    multiple: {
                        items: [
                            { simple: { string: 'hello' } },
                            {
                                message: {
                                    role: 'user',
                                    content: { media: { media: 1, url: 'https://example.test/cat.png' } },
                                    metadataAsJson: '{"source":"test"}',
                                },
                            },
                        ],
                    },
                },
            },
        }).finish();

        const prompt = decodeCallResult(outbound);
        expect(prompt).toBeInstanceOf(BamlPrompt);
        const encoded = encodeCallArgs({ prompt }, {
            callId: 8n,
            functionName: 'user.AcceptPrompt',
        });
        const call = CallFunctionArgs.decode(encoded);
        expect(call.kwargs[0]?.value?.promptAstValue?.multiple?.items).toHaveLength(2);
        expect(call.kwargs[0]?.value?.handle).toBeNull();
    });
});
