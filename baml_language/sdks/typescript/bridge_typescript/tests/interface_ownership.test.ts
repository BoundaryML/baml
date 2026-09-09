// Native ownership probes use an explicit harness codec, separately from generated SDK tests.
import { readFileSync } from 'node:fs';
import { afterAll, afterEach, beforeAll, expect, test } from 'vitest';
import {
    BamlRuntime, BamlHandle, BamlEncodedResult, newFunctionCall,
    shutdownRuntime, _liveHandleCount, _pendingTransferCount,
} from '../dist/native.js';
import { encodeCallArgs, decodeCallResult } from '../dist/proto.js';
import { BamlError } from '../dist/errors.js';
import { BamlInterfaceRef, BamlInterfaceType } from '../dist/interface_ref.js';
import { BamlTypeMap } from '../dist/typemap.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

const source = readFileSync(new URL(
    '../../../../sdk_tests/fixtures/interfaces/baml_src/main.baml', import.meta.url,
), 'utf8');
class HarnessRef extends BamlInterfaceRef {}
const typeMap = BamlTypeMap.fromLazyEntries({
    classes: {}, enums: {}, typeAliases: {},
    interfaces: Object.fromEntries(['Counter', 'ExtendedCounter', 'Greeter', 'CounterCallbackRunner']
        .map(name => [`user.${name}`, () => HarnessRef])),
});
const decode = (result: BamlEncodedResult) => decodeCallResult(result, typeMap);
let runtime: BamlRuntime;
let baseline: number;

function args(name: string, values: Record<string, unknown> = {}): Buffer {
    return encodeCallArgs(values, { callId: BigInt(newFunctionCall()), functionName: name, typeMap });
}
async function call(name: string, values: Record<string, unknown> = {}): Promise<unknown> {
    return decode(await runtime.callFunction(args(name, values)));
}
async function counter(): Promise<HarnessRef> {
    const value = await call('make_counter', { initial: 10 });
    expect(value).toBeInstanceOf(HarnessRef);
    return value as HarnessRef;
}
function invoke(ref: BamlHandle | HarnessRef, member: string, values: Record<string, unknown> = {}): Promise<unknown> {
    // Native preparation runs before returning the Promise, not after awaiting it.
    return (ref instanceof BamlHandle ? ref : ref._toHandle())._callInterfaceMethod(member, args('unused', values)).then(decode);
}
function adoptRaw(result: BamlEncodedResult): BamlHandle {
    const value = baml_bridge.cffi.v1.BamlOutboundResult.decode(result.payload).ok!.handleValue!;
    const handle = result._wrapHandle(value.key!, value.handleType!);
    result._adopt();
    return handle;
}

beforeAll(() => {
    runtime = BamlRuntime.initializeRuntime('.', { 'main.baml': source });
    baseline = _liveHandleCount();
});
afterEach(() => {
    expect(_pendingTransferCount()).toBe(0);
    expect(_liveHandleCount()).toBe(baseline);
});
afterAll(async () => { await shutdownRuntime(); });

test('native interface copies retain one receiver with independent leases', async () => {
    const original = await counter();
    const copy = original.clone();
    expect(Object.isFrozen(original)).toBe(true);
    expect(() => Object.assign(original, { count: 99 })).toThrow(TypeError);
    expect(() => JSON.stringify({ reference: original })).toThrow(/live BAML reference/);
    expect(await invoke(original, 'add', { amount: 2 })).toBe(12);
    const returned = await call('pass_counter', { value: copy }) as HarnessRef;
    expect(await invoke(returned, 'add', { amount: 3 })).toBe(15);
    original.close();
    original.close();
    expect(() => original.clone()).toThrow(/closed/);
    expect(await invoke(copy, 'current')).toBe(15);
    copy.close();
    expect(await invoke(returned, 'current')).toBe(15);
    returned.close();
});

test('local close preserves an already admitted interface call', async () => {
    const ref = await counter();
    const pending = invoke(ref, 'add', { amount: 2 });
    ref.close();
    expect(await pending).toBe(12);
});

test('native interface dispatch uses inherited and default methods', async () => {
    const greeter = await call('make_greeter', { prefix: 'Hello' }) as HarnessRef;
    expect(await invoke(greeter, 'label')).toBe('greeter');
    expect(await invoke(greeter, 'greet', { name: 'Ada' })).toBe('Hello, Ada!');
    greeter.close();
    const extended = await call('make_extended_counter', { initial: 10 }) as HarnessRef;
    expect(await invoke(extended, 'add', { amount: 2 })).toBe(12);
    extended.close();
});

test('invalid methods and arguments reject before mutating the receiver', async () => {
    const ref = await counter();
    await expect(invoke(ref, 'missing')).rejects.toThrow();
    await expect(invoke(ref, 'add', { amount: 'wrong' })).rejects.toThrow();
    expect(await invoke(ref, 'current')).toBe(10);
    ref.close();
});

test('rejected type evidence rolls back receiver leases encoded before it', async () => {
    const ref = await counter();
    class Unregistered extends HarnessRef {}
    const wrongSdk = BamlInterfaceType._create(Unregistered, 'user.Counter', [], []);
    const before = _liveHandleCount();
    expect(() => encodeCallArgs({ value: ref }, {
        callId: BigInt(newFunctionCall()), functionName: 'baml.identity',
        typeArgs: [['T', wrongSdk]], typeMap,
    })).toThrow(/this reference's SDK/);
    expect(_liveHandleCount()).toBe(before);
    expect(await invoke(ref, 'current')).toBe(10);
    ref.close();
});

test('provisional and closed receivers reject and consume argument transfers', async () => {
    const result = await runtime.callFunction(args('make_counter', { initial: 10 }));
    const wire = baml_bridge.cffi.v1.BamlOutboundResult.decode(result.payload).ok!.handleValue!;
    const pending = result._wrapHandle(wire.key!, wire.handleType!);
    await expect(invoke(pending, 'current')).rejects.toThrow(/adopted/);
    result._discard();
    pending.close();

    const ref = await counter();
    const input = await counter();
    ref.close();
    await expect(invoke(ref, 'add', { amount: input })).rejects.toThrow(/closed|reference/);
    input.close();
});

test('closing a provisional wrapper does not lose receipt cleanup', async () => {
    const result = await runtime.callFunction(args('make_counter', { initial: 10 }));
    const wire = baml_bridge.cffi.v1.BamlOutboundResult.decode(result.payload).ok!.handleValue!;
    const wrapper = result._wrapHandle(wire.key!, wire.handleType!);
    wrapper.close();
    result._adopt();
});

test('failed encoding rolls back native clones while retaining the source', async () => {
    const ref = await counter();
    const before = _liveHandleCount();
    expect(() => args('unused', { first: ref, nested: [ref, Symbol('invalid')] })).toThrow(/Cannot encode/);
    expect(_liveHandleCount()).toBe(before);
    expect(await invoke(ref, 'current')).toBe(10);
    ref.close();
});

test.each(['return', 'throw'] as const)('failed callback %s encoding rolls back native clones', async (mode) => {
    let retained: HarnessRef | undefined;
    try {
        await expect(call('visit_counter', { initial: 10, callback: (ref: HarnessRef) => {
            retained = ref;
            const value = { first: ref, nested: [ref, Symbol('invalid')] };
            if (mode === 'throw') throw new BamlError('invalid payload', { value });
            return value;
        } })).rejects.toThrow();
        expect(await invoke(retained!, 'current')).toBe(10);
    } finally {
        retained?.close();
    }
});

test('host callback interface arguments retain callable issuer authority', async () => {
    let retained: HarnessRef | undefined;
    const result = await call('visit_counter', { initial: 10, callback: async (ref: HarnessRef) => {
        retained = ref;
        return await invoke(ref, 'add', { amount: 2 });
    } });
    expect(result).toBe(12);
    expect(await invoke(retained!, 'add', { amount: 3 })).toBe(15);
    retained!.close();
});

test('owned callable admission preserves its receiver after local close', async () => {
    const factory = adoptRaw(await runtime.callFunction(args('make_counter_factory', { initial: 10 })));
    const copy = factory.clone();
    const encoded = encodeCallArgs({}, { callId: BigInt(newFunctionCall()), functionHandle: factory.key });
    const pending = factory._callOwnedFunction(encoded);
    factory.close();
    const ref = decode(await pending) as HarnessRef;
    expect(await invoke(ref, 'add', { amount: 2 })).toBe(12);
    const returned = await call('call_counter_factory', { factory: copy }) as HarnessRef;
    expect(await invoke(returned, 'add', { amount: 3 })).toBe(15);
    copy.close();
    ref.close();
    returned.close();
});

test('interface generic method arguments retain their exact call type bindings', async () => {
    const runner = await call('make_counter_runner', { initial: 10 }) as HarnessRef;
    const encoded = encodeCallArgs({ value: 3, callback: (value: number, optional: { other?: number }) => {
        return value + (optional.other ?? 0);
    } }, {
        callId: BigInt(newFunctionCall()), functionName: 'unused',
        // Method arguments follow the checked declaration's slot order.
        typeArgs: [['', { primitive: { kind: baml_bridge.cffi.v1.BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_INT } }]],
    });
    try {
        expect(decode(await runner._toHandle()._callInterfaceMethod('choose', encoded))).toBe(6);
    } finally {
        runner.close();
    }
});

test('old SDK bindings and references never call a replacement runtime', async () => {
    const ref = await counter();
    const old = runtime;
    runtime = BamlRuntime.initializeRuntime('.', {
        'main.baml': 'function make_counter(initial: int) -> int throws never { initial + 100 }',
    });
    await expect(old.callFunction(args('make_counter', { initial: 1 })).then(decode))
        .rejects.toThrow(/closed or replaced/);
    expect(() => decode(old.callFunctionSync(args('make_counter', { initial: 1 }))))
        .toThrow(/closed or replaced/);
    await expect(invoke(ref, 'current')).rejects.toThrow(/closed or replaced/);
    // Expired bindings consume cloned input leases instead of leaking them.
    await expect(old.callFunction(args('pass_counter', { value: ref })).then(decode))
        .rejects.toThrow(/closed or replaced/);
    expect(await call('make_counter', { initial: 1 })).toBe(101);
    ref.close();
});
