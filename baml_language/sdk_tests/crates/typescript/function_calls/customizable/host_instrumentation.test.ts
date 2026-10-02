/** Shared host contracts mirror test_host_instrumentation.py by test name. */
import { describe, expect, it } from 'vitest';
import { trace } from './baml_sdk/index.js';
// Private carrier access is for execution-context conformance only.
import { _currentExecutionContext as currentExecutionContext } from "@boundaryml/baml-bridge";
import * as baml from './baml_sdk/host_callable_tests/index.js';
import * as contextBaml from './baml_sdk/execution_context_tests/index.js';
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
    expect(trace.current_cancel_token()).toBe(null);
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
      const active = currentExecutionContext()!;
      markEntered();
      try {
        await new Promise<void>(resolve => active.signal.aborted ? resolve()
          : active.signal.addEventListener('abort', () => resolve(), { once: true }));
        markCleaning();
        await release;
        expect(trace.current_context().metadata).toEqual({ request: 7 });
        expect(active.run(() => trace.current_cancel_token()!).is_cancelled()).toBe(true);
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
      expect(trace.current_cancel_token()).toBe(null);
    } finally { unblock(); }
    await exited;
    expect(failures).toEqual([]);
  });

  it('host_baml_cancellation_remains_live_after_exit', async () => {
    const source = CancelToken.new();
    source.cancel();
    let retained: ReturnType<typeof trace.current_cancel_token> = null;
    const work = instrument(async () => {
      retained = trace.current_cancel_token();
      expect(retained!.is_cancelled()).toBe(false);
      await contextBaml.current_context_async({ $baml: { cancel: source } });
    });
    await expect(work()).rejects.toBeInstanceOf(BamlAbortError);
    expect(retained!.is_cancelled()).toBe(true);
    expect(trace.current_cancel_token()).toBe(null);
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
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('host_child_outlives_parent', async () => {
    let unblock!: () => void;
    const release = new Promise<void>(resolve => { unblock = resolve; });
    let retained: ReturnType<typeof currentExecutionContext> = null;
    const parent = instrument(trace.context({ metadata: { request: 7 } }), () => {
      retained = currentExecutionContext();
    });
    const child = instrument(async () => {
      await release;
      return (await contextBaml.current_context_async()).metadata;
    });
    parent();
    expect(trace.current_cancel_token()).toBe(null);
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
    expect(trace.current_cancel_token()).toBe(null);
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
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('host_current_exposes_generated_cancel_token', () => {
    const work = instrument(() => {
      const token = trace.current_cancel_token()!;
      expect(token.is_cancelled()).toBe(false);
      token.cancel();
      expect(token.is_cancelled()).toBe(true);
    });
    work();
    expect(trace.current_cancel_token()).toBe(null);
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
    expect(trace.current_cancel_token()).toBe(null);
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
    expect(trace.current_cancel_token()).toBe(null);
  });

  // SDK_PARITY_LINT(skip): covers JS generator and marker configuration rejection
  it('host_rejects_unsupported_configuration_typescript_only', () => {
    expect(() => instrument(function* () { yield 1; })).toThrow(TypeError);
    expect(() => instrument(trace.span().reserve() as unknown as trace.Options, () => 1)).toThrow(TypeError);
    expect(() => instrument(trace.options(), () => 1, { name: '' })).toThrow(TypeError);
  });

  it('callback_marker_context_precedence', async () => {
    const callback = instrument(trace.context({ distinct_id: 'marker', metadata: { stage: 'marker', keep: 1, remove: 2 } }), async (value: number) => {
      const expected = { stage: 'call', keep: 1, caller: true };
      expect(trace.current_context().distinct_id).toBe('call');
      expect(trace.current_context().metadata).toEqual(expected);
      expect((await contextBaml.current_context_async()).metadata).toEqual(expected);
      return value;
    });
    expect(await baml.call_configured_callback_async(callback as unknown as (value: number) => number, 7,
      trace.context({ distinct_id: 'call', metadata: { stage: 'call', remove: null } }),
      { $baml: { trace: trace.context({ distinct_id: 'caller', metadata: { stage: 'caller', caller: true } }) } })).toBe(7);
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('callback_marker_adopts_sync_body', async () => {
    const callback = instrument(trace.context({ metadata: { stage: 'marker' } }), (value: number) => {
      expect(trace.current_context().metadata).toEqual({ stage: 'call' });
      return value;
    });
    expect(await baml.call_configured_callback_async(callback, 7, trace.context({ metadata: { stage: 'call' } }))).toBe(7);
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('callback_marker_defaults_override_inherited_context', async () => {
    const callback = instrument(trace.context({ metadata: { stage: 'marker' } }), async (value: number) => {
      expect(trace.current_context().metadata).toEqual({ stage: 'marker', keep: 1 });
      return value;
    });
    expect(await baml.call_int_callback_async(callback as unknown as (value: number) => number, 7,
      { $baml: { trace: trace.context({ metadata: { stage: 'caller', keep: 1 } }) } })).toBe(7);
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('callback_marker_adopts_once_during_recursion', async () => {
    const stages: unknown[] = [];
    const callback = instrument(trace.context({ metadata: { stage: 'marker' } }), async (value: number): Promise<number> => {
      stages.push(trace.current_context().metadata.stage);
      if (value) {
        const child = await callback(value - 1);
        expect(trace.current_context().metadata.stage).toBe('call');
        return child + 1;
      }
      return 0;
    });
    expect(await baml.call_configured_callback_async(callback as unknown as (value: number) => number, 1,
      trace.context({ metadata: { stage: 'call' } }))).toBe(1);
    expect(stages).toEqual(['call', 'marker']);
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('callback_marker_only_outer_wrapper_adopts', async () => {
    const inner = instrument(trace.context({ metadata: { stage: 'inner' } }), async (value: number) => {
      expect(trace.current_context().metadata).toEqual({ stage: 'inner', outer: true, call: true });
      return value;
    });
    const outer = instrument(trace.context({ metadata: { stage: 'outer', outer: true } }), inner);
    expect(await baml.call_configured_callback_async(outer as unknown as (value: number) => number, 7,
      trace.context({ metadata: { stage: 'call', call: true } }))).toBe(7);
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('callback_marker_concurrent_reuse_and_later_direct_call', async () => {
    let markReady!: () => void, unblock!: () => void;
    const ready = new Promise<void>(resolve => { markReady = resolve; });
    const release = new Promise<void>(resolve => { unblock = resolve; });
    let entered = 0;
    const callback = instrument(trace.context({ metadata: { stage: 'marker' } }), async (value: number) => {
      if (value < 2) { if (++entered === 2) markReady(); await release; }
      expect(trace.current_context().metadata).toEqual({ stage: value === 2 ? 'marker' : value });
      return value;
    });
    const pending = [0, 1].map(index => baml.call_configured_callback_async(callback as unknown as (value: number) => number,
      index, trace.context({ metadata: { stage: index } })));
    try { await ready; } finally { unblock(); }
    expect(await Promise.all(pending)).toEqual([0, 1]);
    expect(await callback(2)).toBe(2);
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('callback_third_party_wrapper_is_not_adopted', async () => {
    const marked = instrument(trace.context({ metadata: { stage: 'marker' } }), async (value: number) => {
      expect(trace.current_context().metadata).toEqual({ stage: 'marker' });
      return value;
    });
    const thirdParty = async (value: number) => {
      expect(trace.current_context().metadata).toEqual({ stage: 'call' });
      return marked(value);
    };
    Object.assign(thirdParty, marked);
    expect(await baml.call_configured_callback_async(thirdParty as unknown as (value: number) => number, 7,
      trace.context({ metadata: { stage: 'call' } }))).toBe(7);
    expect(trace.current_cancel_token()).toBe(null);
  });

  it('unmarked_callback_explicit_context_and_reentry', async () => {
    const callback = async (value: number) => {
      expect(trace.current_context().metadata).toEqual({ stage: 'call' });
      const child = await contextBaml.current_context_async({ $baml: { trace: trace.context({ metadata: { stage: 'child' } }) } });
      expect(child.metadata).toEqual({ stage: 'child' });
      expect(trace.current_context().metadata).toEqual({ stage: 'call' });
      return value;
    };
    expect(await baml.call_configured_callback_async(callback as unknown as (value: number) => number, 7,
      trace.context({ metadata: { stage: 'call' } }))).toBe(7);
    expect(trace.current_cancel_token()).toBe(null);
  });
});
