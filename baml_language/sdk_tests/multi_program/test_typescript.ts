import assert from 'node:assert/strict';
import { BamlRuntime, callFunctionSync, cancelFunctionCall, decodeCallResult, encodeCallArgs, newFunctionCall } from '@boundaryml/baml-bridge';
import * as a from './a/typescript/baml_sdk/index.js';
import { BYTECODE as bytesA, PROGRAM_KEY as keyA } from './a/typescript/baml_sdk/_inlinedbaml.js';
import { BYTECODE as bytesB, PROGRAM_KEY as keyB } from './b/typescript/baml_sdk/_inlinedbaml.js';

// Fail if native teardown terminates the process before the assertions finish.
process.exitCode = 1;

const closure = a.closure();
const stream = a.stream();
const again = await import('./a_copy/typescript/baml_sdk/index.js');
assert.equal(again.value(), 11);
assert.ok(again.result() instanceof again.Result);
assert.notEqual(again.Result, a.Result);
assert.ok(a.result() instanceof a.Result);
const b = await import('./b/typescript/baml_sdk/index.js');
assert.equal(typeof keyA, 'bigint');
assert.ok(keyA > BigInt(Number.MAX_SAFE_INTEGER) && keyB > BigInt(Number.MAX_SAFE_INTEGER));
assert.notEqual(keyA, keyB);
assert.equal(a.value(), 11);
assert.equal(b.value(), 22);
assert.equal(closure(), 11);
assert.equal(stream.next(), 11);
assert.equal(b.stream().final(), 22);
assert.equal(stream.final(), 11);
const streams = await Promise.all([a.stream_async(), b.stream_async()]);
assert.deepEqual(await Promise.all(streams.map(s => s.finalAsync())), [11, 22]);
assert.ok(a.result() instanceof a.Result);
assert.ok(b.result() instanceof b.Result);
assert.equal(a.result().read(), 11);
assert.equal(b.result().read(), 22);
const repeated = BamlRuntime.initializeRuntimeFromBytecode(Buffer.from(bytesA), undefined, keyA);
assert.equal(repeated.runtimeKey, keyA);
assert.ok(callFunctionSync(repeated, 'user.result', {}).result() instanceof a.Result);
const cancelledCall = BigInt(newFunctionCall());
assert.equal(cancelFunctionCall(cancelledCall), true);
assert.throws(() => decodeCallResult(repeated.callFunctionSync(encodeCallArgs({}, {functionName: 'user.value', callId: cancelledCall}))), /cancel/i);
assert.equal(b.value(), 22);

assert.throws(() => BamlRuntime.initializeRuntimeFromBytecode(Buffer.from(bytesB), undefined, keyA), /Conflicting BAML program/);
const high = BamlRuntime.initializeRuntimeFromBytecode(Buffer.from(bytesA), undefined, (1n << 64n) - 1n);
assert.equal(high.runtimeKey, (1n << 64n) - 1n);
const invoke = (runtime: BamlRuntime, name: string) => decodeCallResult(runtime.callFunctionSync(encodeCallArgs({}, {functionName: name, callId: BigInt(newFunctionCall())})));
assert.equal(invoke(high, 'user.value'), 11);
assert.throws(() => BamlRuntime.initializeRuntimeFromBytecode(Buffer.from(bytesA), undefined, 1n << 64n), /uint64/);
assert.throws(() => BamlRuntime.initializeRuntimeFromBytecode(Buffer.from(bytesA), undefined, -1n), /uint64/);
const values = await Promise.all([
  a.result_async(), b.result_async(),
  a.callback_async((async (value: a.Result) => { assert.ok(value instanceof a.Result); await new Promise(resolve => setTimeout(resolve, 10)); return value; }) as unknown as (value: a.Result) => a.Result),
  b.callback_async((value) => { assert.ok(value instanceof b.Result); return value; }),
]);
assert.deepEqual(values.map(v => v.value), [11,22,11,22]);
assert.ok(values[2] instanceof a.Result && values[3] instanceof b.Result);
const dynamicA = BamlRuntime.initializeRuntime('.', {'main.baml':'function value() -> int { 33 }'});
const dynamicB = BamlRuntime.initializeRuntime('.', {'main.baml':'function value() -> int { 44 }'});
const same = BamlRuntime.initializeRuntime('.', {'main.baml':'function value() -> int { 33 }'});
assert.equal(new Set([dynamicA.runtimeKey, dynamicB.runtimeKey, same.runtimeKey]).size, 3);
assert.equal(invoke(dynamicA, 'value'), 33);
assert.equal(invoke(dynamicB, 'value'), 44);
dynamicA.close();
assert.throws(() => invoke(dynamicA, 'value'), /Unknown BAML runtime/);
assert.equal(invoke(dynamicB, 'value'), 44);
dynamicB.close(); same.close();


const pendingRuntime = BamlRuntime.initializeRuntime('.', {'main.baml': 'function wait(cb: () -> int throws never) -> int { cb() }'});
let release!: (value: number) => void;
let started!: () => void;
const startedPromise = new Promise<void>(resolve => { started = resolve; });
const pending = pendingRuntime.callFunction(encodeCallArgs({ cb: () => new Promise<number>(resolve => { release = resolve; started(); }) }, { functionName: 'wait', callId: BigInt(newFunctionCall()) }));
await startedPromise;
pendingRuntime.close();
release(55);
assert.equal(decodeCallResult(await pending), 55);

const statefulSource = {'main.baml': 'function counter() -> () -> int throws never { let current = 0; () => { current += 1; current } }'};
const statefulA = BamlRuntime.initializeRuntime('.', statefulSource);
const statefulB = BamlRuntime.initializeRuntime('.', statefulSource);
const firstCounter = callFunctionSync(statefulA, 'counter', {}).result() as () => number;
const secondCounter = callFunctionSync(statefulB, 'counter', {}).result() as () => number;
assert.deepEqual([firstCounter(), firstCounter(), secondCounter(), firstCounter(), secondCounter()], [1,2,1,3,2]);
statefulA.close(); statefulB.close();
process.exitCode = 0;
console.log('TypeScript multiple-program regression passed');
