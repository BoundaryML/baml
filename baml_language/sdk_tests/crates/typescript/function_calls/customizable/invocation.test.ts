/** BEP-81 public invocation contracts. Shared names mirror Python. */
import { describe, expect, it } from 'vitest';
import { BamlAbortError } from '@boundaryml/baml-bridge';
import { hello_world, hello_world_async, optional_args_probe, optional_args_probe_async, OptBox, trace, invocation, invoke, invokeAsync, type BamlOptions } from './baml_sdk/index.js';
import { CancelToken } from './baml_sdk/baml/spawn/index.js';
import * as baml from './baml_sdk/host_callable_tests/index.js';
import { isTestRuntime } from './test_runtime.js';

// Web callback continuations use explicit scopes. Node inherits across await.
const asyncCallback = (body: (value: number) => Promise<number>): ((value: number) => number) => body as unknown as (value: number) => number;

describe('invocation_options', () => {
  it('four_call_forms', () => {
    const options: BamlOptions = { timeoutMs: 1000 };
    expect(optional_args_probe(1)).toEqual([1, 5, 99]);
    expect(optional_args_probe(1, { opt1: 7 })).toEqual([1, 7, 99]);
    expect(optional_args_probe(1, { $baml: options })).toEqual([1, 5, 99]);
    expect(optional_args_probe(1, { opt1: 7, $baml: options })).toEqual([1, 7, 99]);
  });
  it('four_call_forms_async', async () => {
    const options: BamlOptions = { timeoutMs: 1000 };
    expect(await optional_args_probe_async(1)).toEqual([1, 5, 99]);
    expect(await optional_args_probe_async(1, { opt1: 7 })).toEqual([1, 7, 99]);
    expect(await optional_args_probe_async(1, { $baml: options })).toEqual([1, 5, 99]);
    expect(await optional_args_probe_async(1, { opt1: 7, $baml: options })).toEqual([1, 7, 99]);
  });
  it('empty_controls', () => {
    for (const options of [undefined, {}, null, { trace: null, cancel: null, timeoutMs: null }]) {
      expect(hello_world({ $baml: options })).toBe('hello world');
    }
  });
  it('omitted_argument_is_not_null', () => {
    expect(optional_args_probe(1, { $baml: {} })).toEqual([1, 5, 99]);
    expect(optional_args_probe(1, { opt1: null, $baml: {} })).toEqual([1, null, 99]);
  });
  it('unknown_control_rejected', () => {
    expect(() => hello_world({ $baml: { unknown: true } as BamlOptions })).toThrow(/unknown/);
  });
  it('invalid_timeout_rejected', () => {
    for (const timeoutMs of [-1, .5, Infinity, NaN, 2147483648]) {
      expect(() => hello_world({ $baml: { timeoutMs } })).toThrow(/timeoutMs/);
    }
  });
  it('timeout_upper_bound_accepted', () => {
    expect(hello_world({ $baml: { timeoutMs: 2147483647 } })).toBe('hello world');
  });
  it.each([[], 3, { cancel: {} }, { trace: {} }])('invalid_controls_rejected %j', (options: unknown) => {
    expect(() => hello_world({ $baml: options as BamlOptions })).toThrow();
  });
  it('legacy_controls_rejected', () => {
    for (const name of ['$ctx', '$trace', '$call']) {
      expect(() => (hello_world as (...args: unknown[]) => unknown)({ [name]: null })).toThrow(/unknown optional argument/);
    }
  });
});

describe('invocation_surfaces', () => {
  it('methods_accept_controls', () => {
    const box = OptBox.make(1, { $baml: {} });
    expect(box.base).toBe(8);
    expect(box.probe(2, { $baml: {} })).toEqual([8, 2, 5]);
    expect(box.probe(2, { opt1: null, $baml: {} })).toEqual([8, 2, null]);
  });
  it('methods_accept_controls_async', async () => {
    const box = await OptBox.make_async(1, { $baml: {} });
    expect(await box.probe_async(2, { $baml: {} })).toEqual([8, 2, 5]);
  });
  it('returned_callable_accepts_controls', async () => {
    const add = baml.make_adder(3);
    expect(add(4, { $baml: {} })).toBe(7);
    expect(await add.callAsync(4, { $baml: {} })).toBe(7);
  });
  it('dynamic_call_accepts_controls', () => {
    expect(invoke('user.optional_args_probe', { arg0: 1 }, { $baml: {} })).toEqual([1, 5, 99]);
    expect(invoke('user.optional_args_probe', { arg0: 1, opt1: null }, { $baml: {} })).toEqual([1, null, 99]);
  });
  it('dynamic_call_async_accepts_controls', async () => {
    expect(await invokeAsync('user.optional_args_probe', { arg0: 1 }, { $baml: {} })).toEqual([1, 5, 99]);
    expect(await invokeAsync(baml.make_adder(3), { value: 4 }, { $baml: {} })).toBe(7);
  });
  it('specialized_callable_rejects_type_bindings', () => {
    const add = baml.make_adder(3);
    expect(() => (add as (...args: unknown[]) => unknown)(4, { $types: { T: 'int' } })).toThrow(/specialized/);
    expect(() => invoke(add, { value: 4 }, { $types: { T: 'int' } })).toThrow(/specialized/);
  });
  it('dynamic_application_map_preserves_control_like_keys', () => {
    expect(() => invoke('user.hello_world', { $baml: null }, { $baml: {} })).toThrow(/\$baml/);
  });
});

describe('invocation_lifecycle', () => {
  it('pre_cancelled_call_does_not_enter_callback', async () => {
    const token = CancelToken.new(); token.cancel();
    const calls: number[] = [];
    await expect(baml.call_int_callback_async(value => { calls.push(value); return value; }, 1, { $baml: { cancel: token } })).rejects.toBeInstanceOf(BamlAbortError);
    expect(calls).toEqual([]);
  });
  it('zero_timeout_does_not_enter_callback', async () => {
    const calls: number[] = [];
    await expect(baml.call_int_callback_async(value => { calls.push(value); return value; }, 1, { $baml: { timeoutMs: 0 } })).rejects.toBeInstanceOf(BamlAbortError);
    expect(calls).toEqual([]);
  });
  it('composite_token_observes_every_source', async () => {
    for (const index of [0, 1]) {
      const sources = [CancelToken.new(), CancelToken.new()];
      const token = CancelToken.any(sources); sources[index]!.cancel();
      expect(token.is_cancelled()).toBe(true);
      await expect(hello_world_async({ $baml: { cancel: token } })).rejects.toBeInstanceOf(BamlAbortError);
      expect(sources[1-index]!.is_cancelled()).toBe(false);
    }
  });
  it('child_cancellation_does_not_cancel_input', async () => {
    const source = CancelToken.new();
    try {
      await baml.call_int_callback_async(value => {
        const active = invocation.current()!;
        active.cancel.cancel();
        expect(active.cancel.is_cancelled()).toBe(true);
        expect(source.is_cancelled()).toBe(false);
        return value;
      }, 1, { $baml: { cancel: source } });
    } catch (error) { expect(error).toBeInstanceOf(BamlAbortError); }
    expect(source.is_cancelled()).toBe(false);
    expect(await hello_world_async({ $baml: { cancel: source } })).toBe('hello world');
  });
  it('rejected_admission_does_not_consume_reservation', async () => {
    const reserved = trace.span().reserve();
    const token = CancelToken.new(); token.cancel();
    await expect(hello_world_async({ $baml: { trace: reserved, cancel: token } })).rejects.toBeInstanceOf(BamlAbortError);
    expect(hello_world({ $baml: { trace: reserved } })).toBe('hello world');
  });
  it('concurrent_reservation_attaches_once', async () => {
    const reserved = trace.span().reserve();
    const results = await Promise.allSettled([hello_world_async({ $baml: { trace: reserved } }), hello_world_async({ $baml: { trace: reserved } })]);
    expect(results.filter(result => result.status === 'fulfilled')).toHaveLength(1);
    expect(results.filter(result => result.status === 'rejected')).toHaveLength(1);
  });
  it('live_token_cancels_after_admission', async () => {
    const token = CancelToken.new();
    let entered!: () => void; const started = new Promise<void>(resolve => { entered = resolve; });
    let release!: () => void; const blocked = new Promise<void>(resolve => { release = resolve; });
    let exited = false;
    const call = baml.call_int_callback_async(asyncCallback(async value => { entered(); await blocked; exited = true; return value; }), 1, { $baml: { cancel: token } });
    try { await started; token.cancel(); await expect(call).rejects.toBeInstanceOf(BamlAbortError); expect(exited).toBe(false); }
    finally { release(); await call.catch(() => {}); }
  });
  it.each([false, true])('retained_effective_token_stays_live_after_callback %s', async (retainFrame: boolean) => {
    const source = CancelToken.new();
    let captured: ReturnType<typeof invocation.current>;
    await baml.call_int_callback_async(value => { captured = invocation.current(); return value; }, 1, { $baml: { cancel: source } });
    expect(invocation.current()).toBe(null);
    const token = retainFrame ? undefined : captured!.cancel;
    source.cancel();
    expect((token ?? captured!.cancel).is_cancelled()).toBe(true);
    expect(captured!.cancel).toBe(captured!.cancel);
    await new Promise<void>(resolve => captured!.signal.aborted ? resolve() : captured!.signal.addEventListener('abort', () => resolve(), { once: true }));
  });
  it('deadline_reentry_does_not_reset_budget', async () => {
    let nested!: Promise<number>;
    const callback = invocation.withInvocation(async (active, value: number) => {
      await new Promise(resolve => setTimeout(resolve, 40));
      nested = active.run(() => baml.call_int_callback_async(asyncCallback(async inner => {
        await new Promise(resolve => setTimeout(resolve, 500)); return inner;
      }), value, { $baml: { timeoutMs: 1000 } }));
      return await nested;
    });
    await expect(baml.call_int_callback_async(callback as unknown as (value: number) => number, 1, { $baml: { timeoutMs: 100 } })).rejects.toBeInstanceOf(BamlAbortError);
    await expect(nested).rejects.toBeInstanceOf(BamlAbortError);
  });
});

describe('invocation_callbacks', () => {
  it('callback_frame_and_reentry', async () => {
    expect(invocation.current()).toBe(null);
    const token = CancelToken.new();
    const callback = invocation.withInvocation(async (active, value: number) => {
      expect(active.cancel.is_cancelled()).toBe(false);
      await Promise.resolve();
      return await active.run(() => baml.call_int_callback_async(inner => { expect(invocation.current()).not.toBe(null); return inner + 1; }, value, { $baml: {} }));
    });
    expect(await baml.call_int_callback_async(callback as unknown as (value: number) => number, 6, { $baml: { cancel: token } })).toBe(7);
    expect(invocation.current()).toBe(null); expect(token.is_cancelled()).toBe(false);
  });
  it('null_controls_preserve_inherited_context', async () => {
    const callback = invocation.withInvocation(async (active, value: number) => active.run(() => baml.call_int_callback_async(() => trace.current_context().metadata.request as number, value, { $baml: { trace: null, cancel: null, timeoutMs: null } })));
    expect(await baml.call_int_callback_async(callback as unknown as (value: number) => number, 1, { $baml: { trace: trace.hidden().context({ metadata: { request: 7 } }) } })).toBe(7);
  });
  it('configuration_snapshot_is_not_live', async () => {
    const options = { trace: trace.context({ metadata: { request: 7 } }) };
    const callback = invocation.withInvocation(async (active, value: number) => {
      options.trace = trace.context({ metadata: { request: 99 } });
      await Promise.resolve();
      expect(active.run(() => trace.current_context().metadata.request)).toBe(7);
      return value;
    });
    expect(await baml.call_int_callback_async(callback as unknown as (value: number) => number, 1, { $baml: options })).toBe(1);
    expect(options.trace.inspect().context.metadata.request).toBe(99);
  });
  it.each([false, true])('layered_callback_context_restores_after_returned_callable %s', async (leafThrows: boolean) => {
    const original = new Error('leaf failed');
    const leaf = invocation.withInvocation(async (active, value: number) => {
      expect(trace.current_context().metadata).toEqual({ request: 'C', keep: 7 });
      await Promise.resolve();
      expect(active.run(() => trace.current_context().metadata)).toEqual({ request: 'C', keep: 7 });
      if (leafThrows) throw original;
      return value + 1;
    });
    const forward = await baml.make_callback_forwarder_async(leaf as unknown as (value: number) => number, { $baml: { trace: trace.context({ metadata: { request: 'factory' } }) } });
    const overridden = invocation.withInvocation(async (active, value: number) => {
      try { return await forward.callAsync(value); }
      finally { expect(active.run(() => trace.current_context().metadata)).toEqual({ request: 'C', keep: 7 }); }
    });
    const inherited = invocation.withInvocation(async (active, value: number) => {
      let result: number;
      try { result = await baml.call_int_callback_async(overridden as unknown as (value: number) => number, value, { $baml: { trace: trace.context({ metadata: { request: 'C' } }) } }); }
      catch (error) { expect(leafThrows).toBe(true); expect(error).toBe(original); result = value + 1; }
      expect(active.run(() => trace.current_context().metadata)).toEqual({ request: 'A', keep: 7 });
      expect(await active.run(() => baml.call_int_callback_async(v => { expect(trace.current_context().metadata).toEqual({ request: 'A', keep: 7 }); return v; }, value))).toBe(value);
      return result;
    });
    const outer = invocation.withInvocation(async (active, value: number) => {
      const result = await baml.call_int_callback_async(inherited as unknown as (value: number) => number, value);
      expect(active.run(() => trace.current_context().metadata)).toEqual({ request: 'A', keep: 7 });
      return result;
    });
    expect(await baml.call_int_callback_async(outer as unknown as (value: number) => number, 6, { $baml: { trace: trace.hidden().context({ metadata: { request: 'A', keep: 7 } }) } })).toBe(7);
    expect(invocation.current()).toBe(null); expect(trace.current_context().metadata).toEqual({});
  });
});

// SDK_PARITY_LINT(skip): Node's ambient async carrier has no Web equivalent.
describe.runIf(isTestRuntime('node'))('invocation_typescript_only', () => {
  it('async_callback_preserves_ambient_frame_typescript_only', async () => {
    expect(await baml.call_int_callback_async(asyncCallback(async value => {
      const active = invocation.current();
      await Promise.resolve();
      expect(invocation.current()).toBe(active);
      expect(trace.current_context().metadata).toEqual({ request: 'A' });
      return value;
    }), 1, { $baml: { trace: trace.context({ metadata: { request: 'A' } }) } })).toBe(1);
  });
});

// SDK_PARITY_LINT(skip): Node async carrier and entry-time snapshot semantics.
describe.runIf(isTestRuntime('node'))('invocation_options_typescript_only', () => {
  it('options_snapshot_at_async_entry_typescript_only', async () => {
    const options = { timeoutMs: 1000 };
    const pending = hello_world_async({ $baml: options });
    options.timeoutMs = 0;
    expect(await pending).toBe('hello world');
  });
  it('native_signal_cancels_without_cancelling_input_token_typescript_only', async () => {
    const source = new AbortController();
    const token = CancelToken.new();
    let entered!: () => void; const started = new Promise<void>(resolve => { entered = resolve; });
    let finished!: () => void; const exited = new Promise<void>(resolve => { finished = resolve; });
    const body = asyncCallback(async value => {
      const active = invocation.current()!;
      entered();
      await new Promise<void>(resolve => active.signal.aborted ? resolve() : active.signal.addEventListener('abort', () => resolve(), { once: true }));
      expect(active.cancel.is_cancelled()).toBe(true);
      finished(); return value;
    });
    const pending = baml.call_int_callback_async(body, 1, { $baml: { cancel: token, signal: source.signal } });
    await started; source.abort();
    await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
    await exited; expect(token.is_cancelled()).toBe(false);
  });
  it('pre_aborted_native_signal_does_not_enter_callback_typescript_only', async () => {
    const source = new AbortController(); source.abort();
    let entered = false;
    await expect(baml.call_int_callback_async(value => { entered = true; return value; }, 1, { $baml: { signal: source.signal } })).rejects.toBeInstanceOf(BamlAbortError);
    expect(entered).toBe(false);
  });
  // Four nested BAML calls; A sets context, B inherits, C patches, D inherits.
  // Intermediate synchronous JS bodies return the child Promise; Node cannot
  // synchronously block its event loop on a child that dispatches JS work.
  for (let modes = 0; modes < 16; modes++) {
    for (const controls of [undefined, {}, null, { trace: null, cancel: null, timeoutMs: null }]) {
      it(`layered_callback_context_inheritance_and_restoration_typescript_only ${modes} ${JSON.stringify(controls)}`, async () => {
        const { AsyncLocalStorage } = await import(/* @vite-ignore */ ['node', 'async_hooks'].join(':')) as typeof import('node:async_hooks');
        const local = new AsyncLocalStorage<string>();
        const rootMetadata = { request: 'A', keep: 7, remove: 9 };
        const childMetadata = { request: 'C', keep: 7, child: 1 };
        const visited: number[] = [];
        const assertLayer = (depth: number) => {
          expect(invocation.current()).not.toBe(null);
          expect(invocation.current()!.cancel.is_cancelled()).toBe(false);
          expect(local.getStore()).toBe('application');
          const context = trace.current_context();
          expect(context.metadata).toEqual(depth >= 2 ? rootMetadata : childMetadata);
          expect(context.distinct_id).toBe(depth >= 2 ? 'root-id' : 'child-id');
        };
        const callbackFor = (depth: number): ((value: number) => number) => {
          const before = () => { assertLayer(depth); visited.push(depth); };
          const after = (value: number) => { assertLayer(depth); return value; };
          const child = (): Promise<number> => depth === 0 ? Promise.resolve(7) : baml.call_int_callback_async(callbackFor(depth - 1), depth - 1, depth === 2 ? { $baml: { trace: trace.context({ distinct_id: 'child-id', metadata: { request: 'C', child: 1, remove: null } }) } } : { $baml: controls });
          if (modes & (1 << depth)) return asyncCallback(async () => { before(); await Promise.resolve(); assertLayer(depth); const value = await child(); await Promise.resolve(); return after(value); });
          return (() => { before(); return child().then(after); }) as unknown as (value: number) => number;
        };
        expect(invocation.current()).toBe(null);
        expect(await local.run('application', () => baml.call_int_callback_async(callbackFor(3), 3, { $baml: { trace: trace.hidden().context({ distinct_id: 'root-id', metadata: rootMetadata }) } }))).toBe(7);
        expect(visited).toEqual([3, 2, 1, 0]); expect(local.getStore()).toBeUndefined();
        expect(invocation.current()).toBe(null); expect(trace.current_context().metadata).toEqual({}); expect(trace.current_context().distinct_id).toBe(null);
      });
    }
  }
});
