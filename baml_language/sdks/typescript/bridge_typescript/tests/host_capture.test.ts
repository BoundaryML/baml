import { describe, expect, it } from 'vitest';
import { capture, registerCapture } from '../typescript_src/host_capture.js';
import { BamlError } from '../typescript_src/errors.js';

function decode(observation: [string, any?]): any {
    const [kind, value] = observation;
    if (kind === 'map') return Object.fromEntries(value.map(([key, item]: [string, [string, any?]]) => [key, decode(item)]));
    if (kind === 'list') return value.map(decode);
    if (kind === 'null') return null;
    if (kind === 'unavailable' || kind === 'depth' || kind === 'values' || kind === 'bytes') return { unavailable: kind };
    return value;
}

const recorded = (value: unknown) => decode(JSON.parse(capture(value)));

describe('host error capture', () => {
    it('preserves messages, stacks, causes and nonenumerable attributes', () => {
        class Failure extends Error {}
        const cause = new TypeError('cause');
        const error = new Failure('failure', { cause });
        Object.defineProperty(error, 'code', { value: 42 });
        const previous = Object.getOwnPropertyDescriptor(Error, 'prepareStackTrace');
        Reflect.deleteProperty(Error, "prepareStackTrace");
        let result: any;
        try { result = recorded(error); }
        finally { if (previous) Object.defineProperty(Error, 'prepareStackTrace', previous); }
        expect(result.type).toBe('Failure');
        expect(result.message).toBe('failure');
        expect(result.stack).toContain('failure');
        expect(result.cause.type).toBe('TypeError');
        expect(result.cause.message).toBe('cause');
        expect(result.attributes.code).toBe(42);
    });

    it('preserves aggregate members and structured BAML error fields', () => {
        class Detail {
            message = 'bad response';
            get dangerous(): never { throw new Error('property must not run'); }
            toJSON(): never { throw new Error('serializer must not run'); }
        }
        registerCapture(Detail, value => ({ message: value.message }));
        const error = new BamlError('parse failed', {
            className: 'ai.errors.ParseFailed', value: new Detail(), bamlTrace: ['frame'],
        });
        const result = recorded(new AggregateError([error], 'failures'));
        expect(result.errors[0].attributes).toEqual({
            value: { message: 'bad response' }, bamlTrace: ['frame'], className: 'ai.errors.ParseFailed',
        });
    });

    it('does not invoke getters, proxies or custom stack formatters', () => {
        let hooks = 0;
        const error = new Error('failure');
        Object.defineProperty(error, 'extra', { get() { hooks++; throw new Error('getter'); } });
        const previous = Object.getOwnPropertyDescriptor(Error, 'prepareStackTrace');
        Error.prepareStackTrace = () => { hooks++; throw new Error('formatter'); };
        try {
            const result = recorded(error);
            expect(result.message).toBe('failure');
            expect(result.attributes.extra).toEqual({ unavailable: 'unavailable' });
            expect(result.stack).toEqual({ unavailable: 'unavailable' });
            expect(recorded(new Proxy(error, { ownKeys() { hooks++; throw new Error('proxy'); } })))
                .toEqual({ unavailable: 'unavailable' });
            expect(hooks).toBe(0);
        } finally {
            if (previous) Object.defineProperty(Error, 'prepareStackTrace', previous);
            else Reflect.deleteProperty(Error, "prepareStackTrace");
        }
    });

    it('bounds cyclic causes, shared references and oversized messages', () => {
        const error = new Error('failure');
        Object.defineProperty(error, 'cause', { value: error });
        expect(recorded(error).cause).toEqual({ unavailable: 'unavailable' });
        expect(recorded([error, error])[1].message).toBe('failure');
        expect(recorded(new Error('x'.repeat(65537))).message).toEqual({ unavailable: 'bytes' });
    });

    it('keeps registered exception projections ahead of builtin capture', () => {
        class Failure extends Error {}
        registerCapture(Failure, value => ({ custom: value.message }));
        expect(recorded(new Failure('projected'))).toEqual({ custom: 'projected' });
    });

    it('captures common containers and scalars without application hooks', () => {
        expect(recorded(new Map([['key', 7]]))).toEqual({ $map: [['key', 7]] });
        expect(recorded(new Set([1, 2]))).toEqual({ $set: [1, 2] });
        expect(recorded(12345678901234567890n)).toEqual({ $bigint: '12345678901234567890' });
        class Bytes extends Uint8Array {
            get buffer(): never { throw new Error('getter must not run'); }
            [Symbol.iterator](): never { throw new Error('iterator must not run'); }
        }
        expect(recorded(new Bytes([97, 98, 99]))).toEqual({ encoding: 'hex', data: '616263' });
        expect(recorded(new URL('https://user:password@example.com/path?secret=value#fragment')))
            .toBe('https://example.com/path');
        const cyclic = new Map(); cyclic.set('self', cyclic);
        expect(recorded(cyclic)).toEqual({ $map: [['self', { unavailable: 'unavailable' }]] });
    });

    it('distinguishes undefined, null, holes, symbols and non-finite numbers', () => {
        expect(recorded([undefined, null, , NaN, Infinity, -Infinity, -0])).toEqual([
            { $undefined: true }, null, { $hole: true }, { $number: 'NaN' },
            { $number: 'Infinity' }, { $number: '-Infinity' }, { $number: '-0' },
        ]);
        expect(recorded(Symbol.for('key'))).toEqual({ $symbol: { description: 'key', key: 'key' } });
        expect(recorded(Object(7))).toBe(7);
        expect(recorded(Object(false))).toBe(false);
        expect(recorded(Object('text'))).toBe('text');
        expect(recorded(Object(7n))).toEqual({ $bigint: '7' });
        expect(recorded(new DOMException('aborted', 'AbortError')))
            .toEqual({ type: 'DOMException', name: 'AbortError', message: 'aborted', code: 20 });
    });

    it('captures typed arrays, binary views, regular expressions and invalid dates', () => {
        expect(recorded(new Int16Array([-7, 8]))).toEqual({ type: 'Int16Array', values: [-7, 8] });
        expect(recorded(new BigInt64Array([123n]))).toEqual({ type: 'BigInt64Array', values: [{ $bigint: '123' }] });
        const bytes = new Uint8Array([0, 1, 2, 3]);
        expect(recorded(bytes.buffer)).toEqual({ encoding: 'hex', data: '00010203' });
        expect(recorded(new DataView(bytes.buffer, 1, 2))).toEqual({ encoding: 'hex', data: '0102' });
        class Pattern extends RegExp {
            get source(): never { throw new Error('getter must not run'); }
            get flags(): never { throw new Error('getter must not run'); }
        }
        expect(recorded(new Pattern('a+', 'gi'))).toEqual({ $regexp: { source: 'a+', flags: 'gi', lastIndex: 0 } });
        expect(recorded(new Date(NaN))).toEqual({ $date: 'invalid' });
    });

    it('captures fetch summaries without consuming bodies', () => {
        const request = new Request('https://example.com/path?secret=value', { method: 'POST', body: 'private body' });
        expect(recorded(request)).toEqual({ method: 'POST', url: 'https://example.com/path' });
        expect(request.bodyUsed).toBe(false);
        const response = new Response('private response', { status: 403 });
        expect(recorded(response)).toEqual({ status_code: 403, url: null });
        expect(response.bodyUsed).toBe(false);
    });
});
