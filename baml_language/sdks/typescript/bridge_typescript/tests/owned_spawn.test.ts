import { afterEach, expect, test } from 'vitest';
import {
    BamlRuntime, BamlEncodedResult, BamlHandle, newFunctionCall,
    registerUnhandledSpawnErrorCallback, shutdownRuntime,
} from '../dist/native.js';
import { decodeCallResult, encodeCallArgs } from '../dist/proto.js';
import { BamlError } from '../dist/errors.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';
import { BamlTypeMap, getTypeMap, setTypeMap } from '../dist/typemap.js';
import { registerHostOpaque, lookupHostValue, tryRehydrateHostValueByKey } from '../dist/host_value_registry.js';

const source = `
class Failure<T> { first: T, second: T }
function main<T>(value: T) -> int {
    spawn { throw value; };
    1
}
function boxed<T>(value: T) -> int {
    spawn { throw Failure<T> { first: value, second: value }; };
    1
}
`;
const received: BamlEncodedResult[] = [];
let breakHandler = false;
registerUnhandledSpawnErrorCallback((result, cancelled) => {
    expect(cancelled).toBe(false);
    received.push(result);
    if (breakHandler) throw new Error('broken report handler');
});
const saved = getTypeMap();
const tick = () => new Promise<void>(resolve => setImmediate(resolve));
async function until(check: () => boolean): Promise<void> {
    for (let i = 0; i < 1000 && !check(); i++) await tick();
    expect(check()).toBe(true);
}
async function spawnOpaque(name = 'main') {
    const original = { label: 'retained thrown host object' };
    const key = registerHostOpaque(original);
    const rawKey = BigInt(key.high >>> 0) << 32n | BigInt(key.low >>> 0);
    const runtime = BamlRuntime.initializeRuntime('.', {'main.baml': source});
    const request = baml_bridge.cffi.v1.CallFunctionArgs.decode(encodeCallArgs({}, {
        callId: BigInt(newFunctionCall()), functionName: name,
    }));
    request.kwargs.push({stringKey: 'value', value: {handle: {
        key, handleType: baml_bridge.cffi.v1.BamlHandleType.HOST_VALUE_OPAQUE,
    }}});
    expect(decodeCallResult(runtime.callFunctionSync(Buffer.from(
        baml_bridge.cffi.v1.CallFunctionArgs.encode(request).finish(),
    )))).toBe(1);
    await shutdownRuntime();
    await until(() => received.length === 1);
    return {original, rawKey, result: received[0]};
}
afterEach(async () => {
    for (const result of received.splice(0)) result._discard();
    breakHandler = false;
    setTypeMap(saved);
    await shutdownRuntime();
});

test('unread spawned error retains its host value until discard after shutdown', async () => {
    const {result, original, rawKey} = await spawnOpaque();
    expect(result).toBeInstanceOf(BamlEncodedResult);
    expect(lookupHostValue(rawKey)).toBe(original);
    result._discard();
    await until(() => lookupHostValue(rawKey) === undefined);
    expect(() => result.payload).toThrow(/consumed/);
});

test('a decoded spawned error adopts its host reference before throwing', async () => {
    const {result, original} = await spawnOpaque();
    let error: BamlError | undefined;
    try { decodeCallResult(result); } catch (caught) { error = caught as BamlError; }
    expect(error).toBeInstanceOf(BamlError);
    const handle = error!.value as BamlHandle;
    expect(handle).toBeInstanceOf(BamlHandle);
    const clone = handle.clone();
    result._discard();
    expect(tryRehydrateHostValueByKey(clone)).toBe(original);
    expect(tryRehydrateHostValueByKey(handle)).toBe(original);
});

test('spawned error model rejection discards even escaped provisional children', async () => {
    const {result, rawKey} = await spawnOpaque('boxed');
    const envelope = baml_bridge.cffi.v1.BamlOutboundResult.decode(result.payload);
    const name = envelope.error!.value!.classValue!.name!;
    let escaped!: BamlHandle;
    class Reject {
        constructor(fields: {first: BamlHandle}) {
            escaped = fields.first;
            expect(() => escaped.clone()).toThrow(/adopted/);
            throw new Error('rejected error model');
        }
    }
    setTypeMap(BamlTypeMap.fromLazyEntries({classes: {[name]: () => Reject}, enums: {}, typeAliases: {}}));
    expect(() => decodeCallResult(result)).toThrow('rejected error model');
    expect(() => escaped.clone()).toThrow(/discarded/);
    await until(() => lookupHostValue(rawKey) === undefined);
});

test('a broken notification handler cannot retain unadopted ownership', async () => {
    breakHandler = true;
    const {result, rawKey} = await spawnOpaque();
    expect(() => result._adopt()).toThrow();
    await until(() => lookupHostValue(rawKey) === undefined);
});
