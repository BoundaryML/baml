// Real Node/native receipt ownership; no mocked native methods or registries.
import { execFileSync } from 'node:child_process';
import { beforeAll, afterAll, afterEach, describe, expect, test } from 'vitest';
import { BamlRuntime, BamlHandle, BamlEncodedResult, BamlImage, BamlAudio, BamlVideo, BamlPdf, newFunctionCall, _pendingTransferCount, shutdownRuntime } from '../dist/native.js';
import { encodeCallArgs, decodeCallResult } from '../dist/proto.js';
import { BamlTypeMap, getTypeMap, setTypeMap } from '../dist/typemap.js';
import { registerHostOpaque, tryRehydrateHostValueByKey } from '../dist/host_value_registry.js';
import { BamlError } from '../dist/errors.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

const source = `
type Unary = (x: int) -> int throws never
function make() -> Unary throws never { (x: int) -> int { x + 1 } }
class Bundle { first: Unary, second: Unary }
function bundle() -> Bundle throws never { Bundle { first: make(), second: make() } }
function fail() -> never throws Bundle { throw bundle() }
function scalar() -> int throws never { 7 }
function throw_null() -> never throws null { throw null }
function opaque<T>(value: T) -> T throws never { value }
function visit(cb: (Bundle) -> int) -> int { cb(bundle()) }
function visit_many(cb: (Bundle, Bundle) -> int) -> int { cb(bundle(), bundle()) }
function visit_image(value: image, cb: (image) -> string) -> string { cb(value) }
function visit_audio(value: audio, cb: (audio) -> string) -> string { cb(value) }
function visit_video(value: video, cb: (video) -> string) -> string { cb(value) }
function visit_pdf(value: pdf, cb: (pdf) -> string) -> string { cb(value) }

`;
let runtime: BamlRuntime;
const saved = getTypeMap();
function args(name: string): Buffer {
    return encodeCallArgs({}, { callId: BigInt(newFunctionCall()), functionName: name });
}
function raw(name: string): BamlEncodedResult { return runtime.callFunctionSync(args(name)); }
function model(ctor: unknown): void {
    setTypeMap(BamlTypeMap.fromLazyEntries({ classes: { 'user.Bundle': () => ctor }, enums: {}, typeAliases: {} }));
}

beforeAll(() => { runtime = BamlRuntime.initializeRuntime('.', { 'main.baml': source }); });
afterEach(() => {
    setTypeMap(saved);
    expect(_pendingTransferCount()).toBe(0);
});
afterAll(async () => { await shutdownRuntime(); });

describe('owned Node result delivery', () => {
    test('unread result can be discarded exactly once', () => {
        const result = raw('bundle');
        expect(result).toBeInstanceOf(BamlEncodedResult);
        expect(_pendingTransferCount()).toBe(1);
        result._discard();
        result._discard();
        expect(() => result.payload).toThrow(/consumed/);
        expect(() => result._adopt()).toThrow(/consumed/);
    });

    test('handles cannot escape a pending or discarded result', () => {
        const result = raw('make');
        const wire = baml_bridge.cffi.v1.BamlOutboundResult.decode(result.payload).ok!.handleValue!;
        const handle = result._wrapHandle(wire.key!, wire.handleType!);
        expect(() => handle.clone()).toThrow(/adopted/);
        result._discard();
        expect(() => handle.key).toThrow(/discarded/);
        expect(() => handle._cloneKeyForWire()).toThrow(/discarded/);
    });

    test('adopted closure remains callable after result consumption', () => {
        const result = raw('make');
        const fn = decodeCallResult(result) as (x: number) => number;
        expect(() => result.payload).toThrow(/consumed/);
        expect(fn(41)).toBe(42);
        expect(fn(1)).toBe(2);
    });

    test('owned host references preserve identity through clone and pass-back', () => {
        const original = { label: 'native payload' };
        const key = registerHostOpaque(original);
        const request = baml_bridge.cffi.v1.CallFunctionArgs.decode(args('opaque'));
        request.kwargs.push({ stringKey: 'value', value: {
            handle: { key, handleType: baml_bridge.cffi.v1.BamlHandleType.HOST_VALUE_OPAQUE },
        } });
        const result = runtime.callFunctionSync(Buffer.from(baml_bridge.cffi.v1.CallFunctionArgs.encode(request).finish()));
        const wire = baml_bridge.cffi.v1.BamlOutboundResult.decode(result.payload).ok!.handleValue!;
        expect(wire.handleType).toBe(baml_bridge.cffi.v1.BamlHandleType.HOST_REFERENCE);
        const handle = decodeCallResult(result) as BamlHandle;
        expect(tryRehydrateHostValueByKey(handle)).toBe(original);
        const copy = handle.clone();
        const passed = runtime.callFunctionSync(encodeCallArgs({value: copy}, {
            callId: BigInt(newFunctionCall()), functionName: 'opaque',
        }));
        const returned = decodeCallResult(passed) as BamlHandle;
        expect(tryRehydrateHostValueByKey(copy)).toBe(original);
        expect(tryRehydrateHostValueByKey(returned)).toBe(original);
    });

    test('callback arguments are adopted and can outlive the invocation', async () => {
        let retained!: {first: (x: number) => number};
        const request = encodeCallArgs({cb: (bundle: {first: (x: number) => number}) => {
            retained = bundle;
            // The native callback has completed argument adoption before entry.
            return bundle.first(5);
        }}, {callId: BigInt(newFunctionCall()), functionName: 'visit'});
        expect(decodeCallResult(await runtime.callFunction(request))).toBe(6);
        expect(retained.first(9)).toBe(10);
    });

    test.each([
        ['image', BamlImage, 'image/png'],
        ['audio', BamlAudio, 'audio/wav'],
        ['video', BamlVideo, 'video/mp4'],
        ['pdf', BamlPdf, 'application/pdf'],
    ] as const)('%s callback retains its concrete payload', async (kind, wrapper, mime) => {
        const url = `https://example.com/${kind}`;
        type Media = BamlImage | BamlAudio | BamlVideo | BamlPdf;
        let original: Media | undefined = wrapper.fromUrl(url, mime);
        let retained!: Media;
        const request = encodeCallArgs({value: original, cb: (value: Media) => {
            expect(value).toBeInstanceOf(wrapper);
            expect(value.mimeType()).toBe(mime);
            retained = value;
            return value.url();
        }}, {callId: BigInt(newFunctionCall()), functionName: `visit_${kind}`});
        expect(decodeCallResult(await runtime.callFunction(request))).toBe(url);
        original = undefined;
        expect(retained.url()).toBe(url);
        expect(retained.mimeType()).toBe(mime);
    });

    test('callback decode failure invalidates escaped refs before entering user code', async () => {
        let escaped!: (x: number) => number;
        let entered = 0;
        let decoded = 0;
        const failure = new Error('callback argument model rejected');
        model(class {
            constructor(fields: {first: (x: number) => number}) {
                decoded++;
                escaped = fields.first;
                if (decoded === 2) throw failure;
            }
        });
        const request = encodeCallArgs({cb: () => { entered++; return 0; }}, {
            callId: BigInt(newFunctionCall()), functionName: 'visit_many',
        });
        const result = await runtime.callFunction(request);
        expect(() => decodeCallResult(result)).toThrow(failure);
        expect(decoded).toBe(2);
        expect(entered).toBe(0);
        expect(() => escaped(1)).toThrow(/discarded/);
    });

    test('async invocation also returns an owned result', async () => {
        const result = await runtime.callFunction(args('make'));
        expect(result).toBeInstanceOf(BamlEncodedResult);
        const fn = decodeCallResult(result) as (x: number) => number;
        expect(fn(4)).toBe(5);
    });

    test('constructor failure invalidates retained nested closures', () => {
        let escaped!: (x: number) => number;
        const failure = new Error('constructor rejected');
        model(class {
            constructor(fields: {first: (x: number) => number}) {
                escaped = fields.first;
                throw failure;
            }
        });
        const result = raw('bundle');
        expect(() => decodeCallResult(result)).toThrow(failure);
        expect(() => escaped(2)).toThrow(/discarded/);
    });

    test('valid declared failure adopts its nested references before throwing', () => {
        const result = raw('fail');
        let error: unknown;
        try { decodeCallResult(result); } catch (e) { error = e; }
        expect(error).toBeInstanceOf(BamlError);
        const fields = (error as BamlError).value as {first: (x: number) => number};
        expect(fields.first(10)).toBe(11);
    });

    test('failure-value constructor errors discard instead of hiding decode failure', () => {
        let escaped!: (x: number) => number;
        const failure = new Error('invalid failure model');
        model(class {
            constructor(fields: {first: (x: number) => number}) {
                escaped = fields.first;
                throw failure;
            }
        });
        expect(() => decodeCallResult(raw('fail'))).toThrow(failure);
        expect(() => escaped(2)).toThrow(/discarded/);
    });

    test('reentrant decoding restores the outer transaction', () => {
        let constructed = 0;
        model(class {
            first: (x: number) => number;
            constructor(fields: {first: (x: number) => number}) {
                constructed++;
                expect(decodeCallResult(raw('scalar'))).toBe(7);
                expect(() => fields.first(1)).toThrow(/adopted/);
                this.first = fields.first;
            }
        });
        const bundle = decodeCallResult(raw('bundle')) as {first: (x: number) => number};
        expect(constructed).toBe(1);
        expect(bundle.first(2)).toBe(3);
    });

    test('throw null is a present failure value', () => {
        let error: unknown;
        try { decodeCallResult(raw('throw_null')); } catch (e) { error = e; }
        expect(error).toBeInstanceOf(BamlError);
        expect((error as BamlError).value).toBeNull();
    });

    test('garbage collection discards an unread result while the runtime stays open', () => {
        const nativeUrl = new URL('../dist/native.js', import.meta.url).href;
        const protoUrl = new URL('../dist/proto.js', import.meta.url).href;
        const wireUrl = new URL('../dist/proto/baml_cffi.js', import.meta.url).href;
        const script = `
            import assert from 'node:assert/strict';
            import { BamlRuntime, BamlHandle, newFunctionCall, _pendingTransferCount, shutdownRuntime } from ${JSON.stringify(nativeUrl)};
            import { encodeCallArgs } from ${JSON.stringify(protoUrl)};
            import { baml_bridge } from ${JSON.stringify(wireUrl)};
            const rt = BamlRuntime.initializeRuntime('.', {'main.baml': ${JSON.stringify(source)}});
            (() => {
                rt.callFunctionSync(encodeCallArgs({}, {functionName: 'bundle', callId: BigInt(newFunctionCall())}));
            })();
            assert.equal(_pendingTransferCount(), 1);
            for (let i = 0; i < 100 && _pendingTransferCount() !== 0; i++) {
                global.gc();
                await new Promise(resolve => setImmediate(resolve));
            }
            assert.equal(_pendingTransferCount(), 0);
            // A JS wrapper may be collected before adoption. Its native owner
            // is rooted by the result, then released if no wrapper survived.
            const result = rt.callFunctionSync(encodeCallArgs({}, {functionName: 'make', callId: BigInt(newFunctionCall())}));
            const wire = baml_bridge.cffi.v1.BamlOutboundResult.decode(result.payload).ok.handleValue;
            const weak = new WeakRef(result._wrapHandle(wire.key, wire.handleType));
            let collected = false;
            for (let i = 0; i < 100; i++) {
                await new Promise(resolve => setImmediate(resolve));
                global.gc();
                await new Promise(resolve => setImmediate(resolve));
                if (!weak.deref()) { collected = true; break; }
            }
            assert.equal(collected, true);
            result._adopt();
            assert.equal(_pendingTransferCount(), 0);
            const probe = new BamlHandle(wire.key, wire.handleType);
            assert.throws(() => probe.clone(), /invalid handle/);
            await shutdownRuntime();
        `;
        execFileSync(process.execPath, ['--expose-gc', '--input-type=module', '-e', script], {timeout: 30000});
    }, 35000);

    test('missing outcome and missing failure value are malformed', () => {
        expect(() => decodeCallResult(Buffer.alloc(0))).toThrow(/no outcome/);
        const malformed = baml_bridge.cffi.v1.BamlOutboundResult.encode({error: {}}).finish();
        expect(() => decodeCallResult(malformed)).toThrow(/no value/);
    });
});
