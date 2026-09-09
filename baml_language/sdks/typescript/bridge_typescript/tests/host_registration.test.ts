// Private native registration/projection operations, with real Node callbacks.
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { BamlRuntime, BamlEncodedResult, _pendingTransferCount, newFunctionCall, shutdownRuntime } from '../dist/native.js';
import { encodeCallArgs, decodeCallResult } from '../dist/proto.js';
import { registerHostOpaque, lookupHostValue } from '../dist/host_value_registry.js';
import { baml_bridge } from '../dist/proto/baml_cffi.js';

const wire = baml_bridge.cffi.v1;
const source = `
interface Echo {
    type Item
    function echo(self, value: Self.Item) -> Self.Item throws never
    function label(self) -> string throws never { "default" }
}`;
let runtime: BamlRuntime;
beforeEach(() => { runtime = BamlRuntime.initializeRuntime('.', { 'main.baml': source }); });
afterEach(async () => {
    expect(_pendingTransferCount()).toBe(0);
    await shutdownRuntime();
});
const bytes = (request: baml_bridge.cffi.v1.IHostOperationRequest) =>
    Buffer.from(wire.HostOperationRequest.encode(request).finish());
const registration = () => bytes({ register: {
    name: 'NodeEcho',
    implementations: [{ interfaceTemplate: { interface: { name: 'user.Echo', bindings: [
        { name: 'Item', ty: { typeVar: { index: 1, name: 'Item' } } },
    ] } }, methods: ['echo'] }],
    typeArgs: [{ typeValue: { primitive: { kind: wire.BamlTyPrimitiveKind.BAML_TY_PRIMITIVE_STRING } } }],
} });

function nativeKey(key: number | { toString(): string }) {
    const value = BigInt(key.toString());
    return { low: Number(BigInt.asIntN(32, value)), high: Number(BigInt.asIntN(32, value >> 32n)) };
}

function adopted(result: BamlEncodedResult) {
    try {
        const output = wire.HostOperationResult.decode(result.payload);
        const handles = output.registered
            ? [output.registered.adapterType!, output.registered.classType!, ...output.registered.interfaceTypes!]
            : output.value ? [output.value.handleValue!] : undefined;
        if (!handles) throw new Error(JSON.stringify(output));
        const owned = handles.map(h => result._wrapHandle(nativeKey(h.key!), h.handleType!));
        result._adopt();
        return { output, owned };
    } catch (error) {
        result._discard();
        throw error;
    }
}

function create(adapter: baml_bridge.cffi.v1.IRegisteredHostAdapter) {
    const receiver = { seen: [] as string[], broken: false, echo(value: string) { this.seen.push(value); return this.broken ? 7 : value; } };
    const receiverKey = registerHostOpaque(receiver);
    const callback = wire.CallFunctionArgs.decode(encodeCallArgs({ callback: receiver.echo.bind(receiver) }, {
        callId: BigInt(newFunctionCall()), functionName: 'unused',
    })).kwargs[0]!.value!;
    const request: baml_bridge.cffi.v1.IHostOperationRequest = { create: {
        adapterType: adapter.adapterType!.key,
        receiver: { handle: wire.BamlHandle.fromObject({ key: receiverKey, handleType: wire.BamlHandleType.HOST_VALUE_OPAQUE }) },
        callbacks: [callback],
    } };
    const key = BigInt(receiverKey.low >>> 0) | (BigInt(receiverKey.high >>> 0) << 32n);
    return { request, key, receiver };
}

test.each(['sync', 'async'])('registration, projection and callback through Node (%s)', async mode => {
    const execute = (request: Buffer) => mode === 'sync' ? runtime._hostOperationSync(request) : runtime._hostOperation(request);
    const type = adopted(await execute(registration()));
    expect(type.output.registered!.adapterType!.handleType).toBe(wire.BamlHandleType.HOST_ADAPTER_TYPE);
    const { request, receiver } = create(type.output.registered!);
    const instance = adopted(await execute(bytes(request)));
    const view = adopted(await execute(bytes({ project: {
        receiver: instance.output.value!.handleValue!.key,
        interfaceType: { typeReference: type.output.registered!.interfaceTypes![0]!.key },
    } })));
    expect(view.output.value!.handleValue!.handleType).toBe(wire.BamlHandleType.ADT_INTERFACE);
    type.owned.forEach(h => h.close());
    instance.owned.forEach(h => h.close());
    for (const [member, arguments_, expected] of [
        ['echo', { value: 'Ada' }, 'Ada'], ['label', {}, 'default'],
    ] as const) {
        const call = wire.CallFunctionArgs.decode(encodeCallArgs(arguments_, { callId: BigInt(newFunctionCall()) }));
        call.interfaceMethod = { view: view.output.value!.handleValue!.key, member };
        expect(decodeCallResult(await runtime.callFunction(Buffer.from(wire.CallFunctionArgs.encode(call).finish())))).toBe(expected);
    }
    expect(receiver.seen).toEqual(['Ada']);
    receiver.broken = true;
    const invalidCall = wire.CallFunctionArgs.decode(encodeCallArgs({ value: 'Ada' }, { callId: BigInt(newFunctionCall()) }));
    invalidCall.interfaceMethod = { view: view.output.value!.handleValue!.key, member: 'echo' };
    const invalidResult = await runtime.callFunction(Buffer.from(wire.CallFunctionArgs.encode(invalidCall).finish()));
    expect(() => decodeCallResult(invalidResult)).toThrow(/HostContractViolation/);
    view.owned.forEach(h => h.close());
});

test('discard invalidates provisional adapter capabilities', () => {
    const result = runtime._hostOperationSync(registration());
    expect(_pendingTransferCount()).toBe(1);
    const registration_ = wire.HostOperationResult.decode(result.payload).registered!;
    const handle = result._wrapHandle(nativeKey(registration_.adapterType!.key!), wire.BamlHandleType.HOST_ADAPTER_TYPE);
    result._discard();
    expect(() => handle.clone()).toThrow(/discarded/);
    expect(_pendingTransferCount()).toBe(0);
});

test.each([false, true])('rejected creation releases host inputs (replaced=%s)', async replaced => {
    const type = adopted(runtime._hostOperationSync(registration()));
    const { request, key } = create(type.output.registered!);
    if (replaced) BamlRuntime.initializeRuntime('.', { 'main.baml': source });
    else request.create!.callbacks!.push(request.create!.callbacks![0]!);
    const result = runtime._hostOperationSync(bytes(request));
    expect(wire.HostOperationResult.decode(result.payload).failure!.error).toBeDefined();
    result._adopt();
    await vi.waitFor(() => expect(lookupHostValue(key)).toBeUndefined());
    type.owned.forEach(h => h.close());
});
