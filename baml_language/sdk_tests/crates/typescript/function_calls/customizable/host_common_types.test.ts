/** Recorded values are independently inspected by the local SQL audit. */
import { expect, it } from 'vitest';
import { trace } from './baml_sdk/index.js';
import { Greeter } from './baml_sdk/methods_on_classes/index.js';
import { GenericBox } from './baml_sdk/generic_tests/index.js';
import * as callbacks from './baml_sdk/host_callable_tests/index.js';
import { isTestRuntime } from './test_runtime.js';

const options = (name: string) => trace.span({ inputs: true, output: true, error: true })
  .context({ metadata: { probe: 'common_types', case: name } });

// SDK_PARITY_LINT(skip): Node observations are checked alongside Python by local SQL.
it.runIf(isTestRuntime('node'))('host_common_types_typescript_only', async () => {
  const native = new Greeter({ name: 'hello' });
  const generic = new GenericBox({ value: 7, $types: { T: 'int' } });
  const date = new Date('2026-10-06T12:30:45Z');
  const failure = new TypeError('problem');
  let hooks = 0;
  const hostileDate = new Date(date);
  hostileDate.toISOString = () => { hooks++; throw new Error('date override'); };
  const getter = { safe: 7, get danger() { hooks++; throw new Error('getter'); } };
  const proxy = new Proxy({}, { ownKeys() { hooks++; throw new Error('proxy'); } });
  const values: Array<[string, unknown]> = [
    ['native', native], ['generic_native', generic], ['date', date],
    ['hostile_date', hostileDate], ['exception_value', failure],
    ['nested', { models: [native], error: failure }], ['getter', getter], ['proxy', proxy],
    ['invalid_date', new Date(NaN)],
    ['value_budget', Array(600).fill(null)], ['byte_budget', 'x'.repeat(64 * 1024 + 1)],
  ];
  for (const [name, value] of values) {
    const identity = trace.instrument(options(name), (value: unknown) => value);
    expect(identity(value)).toBe(value);
  }
  const fail = trace.instrument(options('raised'), () => { throw failure; });
  expect(() => fail()).toThrow(failure);
  const asynchronous = trace.instrument(options('async_native'), async (value: unknown) => value);
  expect(await asynchronous(native)).toBe(native);
  expect(native.who({ $baml: { trace: options('baml_native') } })).toBe('hello');
  expect(generic.get({ $baml: { trace: options('baml_generic') } })).toBe('int');
  const person = new callbacks.Person({ name: 'hello', age: 7 });
  const callback = trace.instrument(options('callback_native'), (person: callbacks.Person) => person);
  expect(await callbacks.call_class_roundtrip_callback_async(callback, person, { $baml: { trace: options('baml_callback') } })).toEqual(person);

  class Custom { constructor(public value: unknown) {} }
  class Derived extends Custom {}
  trace.captureFor(Custom)((value: Custom) => {
    if (value.value === 'failure') throw new Error('projection failure');
    if (value.value === 'cycle') return { safe: 7, cycle: value };
    return { custom: value.value };
  });
  trace.registerCapture(Greeter, () => { throw new Error('native capture takes precedence'); });
  trace.registerCapture(Object, () => { throw new Error('builtin maps take precedence'); });
  for (const [name, value] of [
    ['custom', new Custom(7)], ['custom_subclass', new Derived(7)],
    ['custom_failure', new Custom('failure')], ['custom_cycle', new Custom('cycle')],
    ['native_precedence', native], ['builtin_precedence', { safe: 7 }],
  ] as Array<[string, unknown]>) {
    expect(trace.instrument(options(name), (value: unknown) => value)(value)).toBe(value);
  }
  expect(hooks).toBe(0);
  expect(() => trace.registerCapture(Custom, async value => value)).toThrow(TypeError);
  if (process.env.BAML_HOST_RECORDING_AUDIT === '1') {
    const nativeUrl = new URL('./native.js', import.meta.resolve('@boundaryml/baml-bridge')).href;
    const { shutdownRuntime } = await import(/* @vite-ignore */ nativeUrl);
    await shutdownRuntime();
  }
});
