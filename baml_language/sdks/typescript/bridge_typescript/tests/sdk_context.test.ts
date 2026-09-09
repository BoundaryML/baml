import { afterAll, afterEach, beforeAll, expect, test } from 'vitest';
import { BamlRuntime, newFunctionCall, shutdownRuntime, _pendingTransferCount } from '../dist/native.js';
import { defineFunction } from '../dist/define_function.js';
import { BamlTypeMap, getTypeMap, setTypeMap } from '../dist/typemap.js';
import { decodeCallResult, encodeCallArgs } from '../dist/proto.js';
import { BamlError } from '../dist/errors.js';
import { lowerTypeToWireTy, outboundTyToBamlTypeToken } from '../dist/wire_ty.js';

const source = `
class Payload { text: string }
function make() -> Payload throws never { Payload { text: "Ada" } }
function copy(value: Payload) -> Payload throws never { value }
function closure() -> (value: Payload) -> Payload throws never { (value) -> { value } }
function apply(callback: (Payload) -> Payload throws never, value: Payload) -> Payload throws never { callback(value) }
function visit(callback: (Payload) -> Payload throws never) -> Payload throws never { callback(make()) }
function fail(callback: () -> Payload throws Payload) -> Payload throws Payload { callback() }
function read() -> () -> Payload throws never { () -> { make() } }
`;
class Payload {
    text: string;
    constructor(init: { text: string }) { this.text = init.text; }
}
class OtherPayload extends Payload {}
const typeMap = BamlTypeMap.fromLazyEntries({
    classes: { 'user.Payload': () => Payload }, enums: {}, typeAliases: {},
});
const otherMap = BamlTypeMap.fromLazyEntries({
    classes: { 'user.Payload': () => OtherPayload }, enums: {}, typeAliases: {},
});
const saved = getTypeMap();
let runtime: BamlRuntime;

beforeAll(() => {
    runtime = BamlRuntime.initializeRuntime('.', { 'main.baml': source });
    setTypeMap(typeMap);
});
afterEach(() => {
    setTypeMap(typeMap);
    expect(_pendingTransferCount()).toBe(0);
});
afterAll(async () => { setTypeMap(saved); await shutdownRuntime(); });

test('function factories retain their SDK codecs across global map changes', async () => {
    const make = defineFunction('user.make', 'sync', []);
    const copy = defineFunction('user.copy', 'async', ['value']);
    setTypeMap(otherMap);
    expect(make()).toBeInstanceOf(Payload);
    expect(make()).not.toBeInstanceOf(OtherPayload);
    const result = await copy(new Payload({ text: 'Grace' }));
    expect(result).toBeInstanceOf(Payload);
    expect(result).not.toBeInstanceOf(OtherPayload);
    expect(result).toEqual(new Payload({ text: 'Grace' }));
});

test('returned closures use their captured codecs and native carrier on pass-back', () => {
    const makeClosure = defineFunction('user.closure', 'sync', []);
    const apply = defineFunction('user.apply', 'sync', ['callback', 'value']);
    const closure = makeClosure() as (value: Payload) => Payload;
    setTypeMap(otherMap);
    const value = new Payload({ text: 'Grace' });
    expect(closure(value).constructor).toBe(Payload);
    // A re-registered host callback would fail the synchronous-call guard.
    expect((apply(closure, value) as Payload).constructor).toBe(Payload);
    expect(closure(value).text).toBe('Grace');
});

test('callback arguments and async completions use the registration codecs', async () => {
    const visit = defineFunction('user.visit', 'async', ['callback']);
    setTypeMap(otherMap);
    const result = await visit(async (value: Payload) => {
        expect(value.constructor).toBe(Payload);
        await Promise.resolve();
        return new Payload({ text: `${value.text}!` });
    });
    expect(result).toEqual(new Payload({ text: 'Ada!' }));
    expect((result as Payload).constructor).toBe(Payload);
});

test('declared callback errors use captured codecs for encoding and decoding', async () => {
    const fail = defineFunction('user.fail', 'async', ['callback']);
    setTypeMap(otherMap);
    try {
        await fail(async () => { throw new BamlError('declared', { value: new Payload({ text: 'failure' }) }); });
        throw new Error('expected BAML error');
    } catch (error) {
        expect(error).toBeInstanceOf(BamlError);
        expect((error as BamlError).value).toEqual(new Payload({ text: 'failure' }));
        expect(((error as BamlError).value as Payload).constructor).toBe(Payload);
    }
});

test('nested native type tokens use the explicitly selected SDK', () => {
    setTypeMap(otherMap);
    const wire = lowerTypeToWireTy({ list: Payload }, typeMap);
    expect(wire.list?.item?.classTy?.name).toBe('user.Payload');
    expect(outboundTyToBamlTypeToken(wire, typeMap)).toEqual({ list: Payload });
});

test('an existing factory and returned closure reject a replacement runtime', async () => {
    const make = defineFunction('user.make', 'async', []);
    const read = defineFunction('user.read', 'sync', [])() as () => Payload;
    runtime = BamlRuntime.initializeRuntime('.', { 'main.baml': 'function make() -> int throws never { 99 }' });
    await expect(make()).rejects.toThrow(/closed or replaced/);
    expect(() => read()).toThrow(/closed or replaced/);
    const args = encodeCallArgs({}, { callId: BigInt(newFunctionCall()), functionName: 'make' });
    expect(decodeCallResult(await runtime.callFunction(args))).toBe(99);
});
