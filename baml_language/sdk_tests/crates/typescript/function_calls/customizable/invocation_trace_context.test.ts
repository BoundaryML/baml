/** Shared invocation-context contracts; no instrumentation or trace hooks.
 * Mirrors the Python cases guided by host_tracing.py CTX/ASYNC/CB/ID scenarios.
 */
import { describe, expect, it } from 'vitest';
import { invocation, trace } from './baml_sdk/index.js';
import * as baml from './baml_sdk/host_callable_tests/index.js';
import * as contextBaml from './baml_sdk/invocation_context/index.js';
import { CancelToken } from './baml_sdk/baml/spawn/index.js';
import { BamlAbortError } from '@boundaryml/baml-bridge';

describe('invocation_trace_context', () => {
  it('callback_captures_internal_baml_context', async () => {
    expect(await contextBaml.internal_context_async(value => {
      expect(invocation.current()).not.toBe(null);
      const current = trace.current_context();
      expect(current.distinct_id).toBe('internal-id');
      expect(current.metadata).toEqual({ phase: 'internal', keep: 7 });
      return value;
    }, 7, { $baml: { trace: trace.hidden().context({
      distinct_id: 'root-id', metadata: { phase: 'root', keep: 7, remove: 9 },
    }) } })).toBe(7);
    expect(invocation.current()).toBe(null);
    expect(trace.current_context().metadata).toEqual({});
  });

  it('current_context_returns_detached_snapshot', async () => {
    expect(await baml.call_int_callback_async(value => {
      const observed = trace.current_context();
      observed.metadata.request = 99;
      observed.distinct_id = 'changed';
      expect(trace.current_context().metadata).toEqual({ request: 7 });
      expect(trace.current_context().distinct_id).toBe('original');
      return value;
    }, 1, { $baml: { trace: trace.context({ distinct_id: 'original', metadata: { request: 7 } }) } })).toBe(1);
  });

  it('concurrent_invocations_isolate_context', async () => {
    let markReady!: () => void;
    const ready = new Promise<void>(resolve => { markReady = resolve; });
    let unblock!: () => void;
    const release = new Promise<void>(resolve => { unblock = resolve; });
    let entered = 0;
    const pending = [0, 1].map(index => {
      const callback = invocation.withInvocation(async (active, value: number) => {
        expect(trace.current_context().metadata).toEqual({ request: index });
        if (++entered === 2) markReady();
        await release;
        expect(active.run(() => trace.current_context().metadata)).toEqual({ request: index });
        const nested = await active.run(() => contextBaml.current_context_async());
        expect(nested.metadata).toEqual({ request: index });
        return value;
      });
      return baml.call_int_callback_async(callback as unknown as (value: number) => number, index,
        { $baml: { trace: trace.hidden().context({ metadata: { request: index } }) } });
    });
    try { await ready; } finally { unblock(); }
    expect(await Promise.all(pending)).toEqual([0, 1]);
    expect(invocation.current()).toBe(null);
    expect(trace.current_context().metadata).toEqual({});
  });

  it('retained_context_survives_parent_completion', async () => {
    let captured: ReturnType<typeof invocation.current> = null;
    expect(await baml.call_int_callback_async(value => {
      captured = invocation.current();
      return value;
    }, 1, { $baml: { trace: trace.hidden().context({ metadata: { request: 7 } }) } })).toBe(1);
    expect(invocation.current()).toBe(null);
    const active = captured!;
    expect(active.run(() => trace.current_context().metadata)).toEqual({ request: 7 });
    expect(await active.run(() => baml.call_int_callback_async(value => {
      expect(trace.current_context().metadata).toEqual({ request: 7 });
      return value;
    }, 7))).toBe(7);
    expect(invocation.current()).toBe(null);
    expect(trace.current_context().metadata).toEqual({});
  });

  it('reservation_does_not_inherit', async () => {
    const reserved = trace.span().context({ metadata: { request: 7 } }).reserve();
    const callback = invocation.withInvocation(async (active, value: number) => {
      expect((await active.run(() => contextBaml.current_context_async())).metadata).toEqual({ request: 7 });
      return value;
    });
    expect(await baml.call_int_callback_async(callback as unknown as (value: number) => number, 1,
      { $baml: { trace: reserved } })).toBe(1);
    await expect(contextBaml.current_context_async({ $baml: { trace: reserved } }))
      .rejects.toThrow(/invalid trace reservation: AlreadyAttached/);
  });

  it('hidden_mode_does_not_inherit', async () => {
    expect(await contextBaml.child_span_under_hidden_parent_async({ $baml: { trace: trace.hidden() } })).toBe(true);
  });

  it('cancelled_waiter_preserves_callback_context', async () => {
    const source = CancelToken.new();
    let markStarted!: () => void;
    const started = new Promise<void>(resolve => { markStarted = resolve; });
    let markCleanup!: () => void;
    const cleanup = new Promise<void>(resolve => { markCleanup = resolve; });
    let unblock!: () => void;
    const release = new Promise<void>(resolve => { unblock = resolve; });
    let markExited!: () => void;
    const finished = new Promise<void>(resolve => { markExited = resolve; });
    let exited = false;
    const cleanupErrors: unknown[] = [];
    const callback = invocation.withInvocation(async (active, value: number) => {
      markStarted();
      await new Promise<void>(resolve => active.signal.aborted ? resolve()
        : active.signal.addEventListener('abort', () => resolve(), { once: true }));
      markCleanup();
      try {
        expect(active.run(() => trace.current_context().metadata)).toEqual({ request: 7 });
        expect(active.run(() => active.cancel.is_cancelled())).toBe(true);
        await release;
        expect(active.run(() => trace.current_context().metadata)).toEqual({ request: 7 });
      } catch (error) {
        // The cancelled waiter cannot report failures from physical cleanup.
        cleanupErrors.push(error);
        throw error;
      } finally { exited = true; markExited(); }
      return value;
    });
    const pending = baml.call_int_callback_async(callback as unknown as (value: number) => number, 1,
      { $baml: { cancel: source, trace: trace.hidden().context({ metadata: { request: 7 } }) } });
    try {
      await started; source.cancel();
      await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
      await cleanup;
      expect(exited).toBe(false);
      expect(invocation.current()).toBe(null);
      expect(trace.current_context().metadata).toEqual({});
    } finally { unblock(); }
    await finished;
    expect(cleanupErrors).toEqual([]);
  });
});
