/** Shared host contracts mirror test_host_instrumentation.py by test name. */
import { describe, expect, it } from 'vitest';
import { invocation, trace } from './baml_sdk/index.js';
import * as baml from './baml_sdk/host_callable_tests/index.js';
import * as contextBaml from './baml_sdk/invocation_context/index.js';
import { isTestRuntime } from './test_runtime.js';
import { CancelToken } from './baml_sdk/baml/spawn/index.js';
import { BamlAbortError } from '@boundaryml/baml-bridge';

// The same overlay is compiled for Web, where host instrumentation is not
// exported yet. The runtime gate below restricts execution to Node.
type Body = (this: any, ...args: any[]) => any;
const instrument = (trace as unknown as {
  instrument<F extends Body>(body: F): F;
  instrument<F extends Body>(options: trace.Options | null, body: F, display?: { name?: string }): F;
}).instrument;

describe.runIf(isTestRuntime('node'))('host_instrumentation', () => {
  it('host_context_inherits_and_restores', () => {
    const child = instrument(trace.context({ metadata: { drop: null, child: true } }), () => {
      const observed = contextBaml.current_context();
      expect(observed.distinct_id).toBe('alice');
      expect(observed.metadata).toEqual({ keep: 1, child: true });
      observed.metadata.keep = 99;
      expect(trace.current_context().metadata).toEqual({ keep: 1, child: true });
    });
    const parent = instrument(trace.hidden().context({ distinct_id: 'alice', metadata: { keep: 1, drop: 2 } }), () => {
      child();
      expect(trace.current_context().metadata).toEqual({ keep: 1, drop: 2 });
    });
    parent();
    expect(trace.current_context().metadata).toEqual({});
  });

  it('host_callback_reentry_context', async () => {
    const parent = instrument(trace.context({ metadata: { phase: 'host' } }), async () => {
      const callback = async (value: number) => {
        expect(trace.current_context().metadata).toEqual({ phase: 'callback' });
        expect((await contextBaml.current_context_async()).metadata).toEqual({ phase: 'callback' });
        return value;
      };
      expect(await baml.call_int_callback_async(callback as unknown as (value: number) => number, 7,
        { $baml: { trace: trace.context({ metadata: { phase: 'callback' } }) } })).toBe(7);
      expect(trace.current_context().metadata).toEqual({ phase: 'host' });
    });
    await parent();
    expect(invocation.current()).toBe(null);
  });

  it('host_callback_cleanup_retains_context', async () => {
    const source = CancelToken.new();
    let markEntered!: () => void;
    const entered = new Promise<void>(resolve => { markEntered = resolve; });
    let markCleaning!: () => void;
    const cleaning = new Promise<void>(resolve => { markCleaning = resolve; });
    let unblock!: () => void;
    const release = new Promise<void>(resolve => { unblock = resolve; });
    let markExited!: () => void;
    const exited = new Promise<void>(resolve => { markExited = resolve; });
    let done = false;
    const failures: unknown[] = [];
    const callback = instrument(async (value: number) => {
      const active = invocation.current()!;
      markEntered();
      try {
        await new Promise<void>(resolve => active.signal.aborted ? resolve()
          : active.signal.addEventListener('abort', () => resolve(), { once: true }));
        markCleaning();
        await release;
        expect(trace.current_context().metadata).toEqual({ request: 7 });
        expect(active.cancel.is_cancelled()).toBe(true);
        return value;
      } catch (error) { failures.push(error); throw error; }
      finally { done = true; markExited(); }
    });
    const pending = baml.call_int_callback_async(callback as unknown as (value: number) => number, 7,
      { $baml: { cancel: source, trace: trace.context({ metadata: { request: 7 } }) } });
    try {
      await entered; source.cancel();
      await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
      await cleaning;
      expect(done).toBe(false);
      expect(invocation.current()).toBe(null);
    } finally { unblock(); }
    await exited;
    expect(failures).toEqual([]);
  });

  it('host_baml_cancellation_remains_live_after_exit', async () => {
    const source = CancelToken.new();
    source.cancel();
    let retained: ReturnType<typeof invocation.current> = null;
    const work = instrument(async () => {
      retained = invocation.current();
      expect(retained!.cancel.is_cancelled()).toBe(false);
      await contextBaml.current_context_async({ $baml: { cancel: source } });
    });
    await expect(work()).rejects.toBeInstanceOf(BamlAbortError);
    expect(retained!.cancel.is_cancelled()).toBe(true);
    expect(invocation.current()).toBe(null);
  });

  it('concurrent_host_invocations_isolate_context', async () => {
    let markReady!: () => void;
    const ready = new Promise<void>(resolve => { markReady = resolve; });
    let unblock!: () => void;
    const release = new Promise<void>(resolve => { unblock = resolve; });
    let entered = 0;
    const pending = [0, 1].map(index => {
      const work = instrument(trace.context({ metadata: { request: index } }), async () => {
        if (++entered === 2) markReady();
        await release;
        return (await contextBaml.current_context_async()).metadata;
      });
      return work();
    });
    try { await ready; } finally { unblock(); }
    expect(await Promise.all(pending)).toEqual([{ request: 0 }, { request: 1 }]);
    expect(invocation.current()).toBe(null);
  });

  it('host_child_outlives_parent', async () => {
    let unblock!: () => void;
    const release = new Promise<void>(resolve => { unblock = resolve; });
    let retained: ReturnType<typeof invocation.current> = null;
    const parent = instrument(trace.context({ metadata: { request: 7 } }), () => {
      retained = invocation.current();
    });
    const child = instrument(async () => {
      await release;
      return (await contextBaml.current_context_async()).metadata;
    });
    parent();
    expect(invocation.current()).toBe(null);
    const pending = retained!.run(child);
    unblock();
    expect(await pending).toEqual({ request: 7 });
  });

  it('host_errors_preserve_identity_and_restore_context', () => {
    const failure = new Error('application failure');
    const child = instrument(trace.context({ metadata: { phase: 'child' } }), () => { throw failure; });
    const parent = instrument(trace.context({ metadata: { phase: 'parent' } }), () => {
      try { child(); throw new Error('expected application failure'); }
      catch (error) { expect(error).toBe(failure); }
      expect(trace.current_context().metadata).toEqual({ phase: 'parent' });
    });
    parent();
    expect(invocation.current()).toBe(null);
  });

  it('host_capture_preserves_application_values', () => {
    class Opaque {
      toJSON(): never { throw new Error('capture must not serialize application objects'); }
      *[Symbol.iterator](): Generator { throw new Error('capture must not consume an iterator'); }
    }
    const failure = new Error('application failure');
    const result = new Opaque();
    const cycle: unknown[] = [];
    cycle.push(cycle);
    const work = instrument(trace.span({ inputs: true, output: true, error: true }), (value: unknown) => {
      if (value === failure) throw failure;
      return result;
    });
    expect(work(cycle)).toBe(result);
    expect(work('x'.repeat(100_000))).toBe(result);
    try { work(failure); throw new Error('expected application failure'); }
    catch (error) { expect(error).toBe(failure); }
    expect(invocation.current()).toBe(null);
  });

  it('host_current_exposes_generated_cancel_token', () => {
    const work = instrument(() => {
      const active = invocation.current()!;
      expect(active.cancel.is_cancelled()).toBe(false);
      active.cancel.cancel();
      expect(active.cancel.is_cancelled()).toBe(true);
    });
    work();
    expect(invocation.current()).toBe(null);
  });

  // SDK_PARITY_LINT(skip): covers JS receivers, variadic arguments, and native Promise results
  it('host_preserves_call_shape_typescript_only', async () => {
    const result = {};
    const receiver = {
      prefix: 'request',
      work: instrument(function(this: { prefix: string }, first: number, ...rest: number[]) {
        expect(this.prefix).toBe('request');
        expect([first, ...rest]).toEqual([1, 2, 3]);
        return result;
      }),
    };
    expect(receiver.work(1, 2, 3)).toBe(result);
    const promise = Promise.resolve(result);
    const asynchronous = instrument(() => promise);
    expect(await asynchronous()).toBe(result);
    const failure = new Error('application failure');
    const rejects = instrument(async () => { throw failure; });
    try { await rejects(); throw new Error('expected application failure'); }
    catch (error) { expect(error).toBe(failure); }
    expect(invocation.current()).toBe(null);
  });

  // SDK_PARITY_LINT(skip): covers JS getters, Proxy traps, and custom thenables
  it('host_capture_avoids_getters_and_proxies_typescript_only', () => {
    const value = Object.defineProperty({}, 'field', {
      enumerable: true, get() { throw new Error('getter must not run'); },
    });
    const proxy = new Proxy({}, { ownKeys() { throw new Error('proxy must not run'); } });
    const thenable = Object.defineProperty({}, 'then', { get() { throw new Error('then must not run'); } });
    const work = instrument(trace.span({ inputs: true, output: true }), (value: unknown) => value);
    expect(work(value)).toBe(value);
    expect(work(proxy)).toBe(proxy);
    expect(work(thenable)).toBe(thenable);
    let prototypeInspections = 0;
    const failure = Object.create(new Proxy({}, { getPrototypeOf() {
      prototypeInspections++;
      throw new Error('unsafe exception prototype');
    } }));
    const throwing = instrument(() => { throw failure; });
    try { throwing(); throw new Error('expected application failure'); }
    catch (error) { expect(error).toBe(failure); }
    expect(prototypeInspections).toBe(0);
    expect(invocation.current()).toBe(null);
  });

  // SDK_PARITY_LINT(skip): covers JS generator and marker configuration rejection
  it('host_rejects_unsupported_configuration_typescript_only', () => {
    expect(() => instrument(function* () { yield 1; })).toThrow(TypeError);
    expect(() => instrument(trace.span().reserve() as unknown as trace.Options, () => 1)).toThrow(TypeError);
    expect(() => instrument(trace.options(), () => 1, { name: '' })).toThrow(TypeError);
  });
});
