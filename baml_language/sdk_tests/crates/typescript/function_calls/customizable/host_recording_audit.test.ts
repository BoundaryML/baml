/** Local recording evidence is checked by sdk_tests/audits/host_instrumentation. */
import { expect, it } from 'vitest';
import { trace } from './baml_sdk/index.js';
import * as contextBaml from './baml_sdk/execution_context_tests/index.js';
import * as callbacks from './baml_sdk/host_callable_tests/index.js';
import { isTestRuntime } from './test_runtime.js';

const options = (name: string) => trace.span({ inputs: true, output: true, error: true })
  .context({ distinct_id: 'audit-typescript', metadata: { audit: 'typescript', case: name } });

// SDK_PARITY_LINT(skip): runs the Node half of the independent local SQL audit
it.runIf(isTestRuntime('node'))('host_recording_audit_typescript_only', async () => {
  const ordinary = trace.instrument(options('sync'), (value: number, flag: boolean) => {
    expect(contextBaml.current_context().metadata.case).toBe('sync');
    return { value: value + 1, flag };
  }, { name: 'audit custom sync' });
  expect(ordinary(7, true)).toEqual({ value: 8, flag: true });

  class Service {
    offset = 10;
    plain(value: number) { return this.offset + value; }
    method = trace.instrument(options('method'), function (this: Service, value: number) {
      return this.offset + value;
    }, { name: 'audit method' });
    static make = trace.instrument(options('staticmethod'), function (this: typeof Service, value: number) {
      expect(this).toBe(Service);
      return value + 3;
    },
      { name: 'audit staticmethod' });
  }
  const service = new Service();
  expect(service.method(7)).toBe(17);
  const bound = trace.instrument(options('bound'), service.plain.bind(service), { name: 'audit bound method' });
  expect(bound(7)).toBe(17);
  expect(Service.make(7)).toBe(10);

  const failure = { message: 'audit application failure', code: 7 };
  const fail = trace.instrument(options('error'), () => { throw failure; }, { name: 'audit error' });
  try { fail(); throw new Error('application failure disappeared'); }
  catch (caught) { expect(caught).toBe(failure); }

  const nativeFailure = new Error('audit native Error');
  const nativeFail = trace.instrument(options('native_error'), () => { throw nativeFailure; });
  try { nativeFail(); throw new Error('application failure disappeared'); }
  catch (caught) { expect(caught).toBe(nativeFailure); }

  expect(() => trace.instrument(function* () { yield 1; })).toThrow(TypeError);
  expect(() => trace.instrument(async function* () { yield 1; })).toThrow(TypeError);
  const iterator = [1, 2][Symbol.iterator]();
  const returnIterator = trace.instrument(options('iterator'), () => iterator);
  expect(returnIterator()).toBe(iterator);
  expect([...iterator]).toEqual([1, 2]);

  const asynchronous = trace.instrument(options('async'), async (value: number) => {
    await Promise.resolve();
    expect((await contextBaml.current_context_async()).metadata.case).toBe('async');
    return value + 1;
  }, { name: 'audit custom async' });
  expect(await asynchronous(8)).toBe(9);
  const asyncFail = trace.instrument(options('async_error'), async () => {
    await Promise.resolve();
    throw failure;
  }, { name: 'audit async error' });
  await expect(asyncFail()).rejects.toBe(failure);

  let unblock!: () => void;
  const release = new Promise<void>(resolve => { unblock = resolve; });
  const entered: Array<() => void> = [];
  const ready = [0, 1].map(() => new Promise<void>(resolve => { entered.push(resolve); }));
  const callback = trace.instrument(options('marker').context({ metadata: { keep: 1, drop: 2 } }), async (value: number) => {
    if (value < 2) { entered[value](); await release; }
    const current = trace.current_context();
    expect(current.distinct_id).toBe(value < 2 ? `request-${value}` : 'audit-typescript');
    expect(current.metadata).toEqual(value < 2
      ? { audit: 'typescript', case: `call-${value}`, keep: 1, caller: true }
      : { audit: 'typescript', case: 'marker', keep: 1, drop: 2 });
    expect((await contextBaml.current_context_async()).metadata).toEqual(current.metadata);
    return value + 10;
  }, { name: 'audit reusable callback' });
  const pending = [0, 1].map(value => callbacks.call_configured_callback_async(
    callback as unknown as (value: number) => number, value,
    options(`call-${value}`).context({ distinct_id: `request-${value}`, metadata: { drop: null } }),
    { $baml: { trace: options('caller').context({ metadata: { caller: true } }) } },
  ));
  try { await Promise.all(ready); }
  finally { unblock(); }
  expect(await Promise.all(pending)).toEqual([10, 11]);
  expect(await callback(2)).toBe(12);
  expect(trace.current_context().metadata).toEqual({});
  expect(trace.current_cancel_token()).toBe(null);
  // Vitest terminates worker processes without Node's beforeExit hook. The
  // independent recording audit must await the real runtime shutdown writer.
  if (process.env.BAML_HOST_RECORDING_AUDIT === '1') {
    const nativeUrl = new URL('./native.js', import.meta.resolve('@boundaryml/baml-bridge')).href;
    const { shutdownRuntime } = await import(/* @vite-ignore */ nativeUrl);
    await shutdownRuntime();
  }
});
