/** Shared invocation-context contracts; no instrumentation or trace hooks.
 * Mirrors the Python cases guided by host_tracing.py CTX/ASYNC/CB/ID scenarios.
 */
import { describe, expect, it } from 'vitest';
import { trace } from './baml_sdk/index.js';
// Private carrier access is for execution-context conformance only.
import { _currentExecutionContext as currentExecutionContext, _withExecutionContext as withExecutionContext } from "@boundaryml/baml-bridge";
import * as baml from './baml_sdk/host_callable_tests/index.js';
import * as contextBaml from './baml_sdk/execution_context_tests/index.js';
import { CancelToken } from './baml_sdk/baml/spawn/index.js';
import { BamlAbortError } from '@boundaryml/baml-bridge';

describe('invocation_trace_context', () => {
  it('callback_captures_internal_baml_context', async () => {
    expect(await contextBaml.internal_context(value => {
      expect(trace.current_cancel_token()).not.toBe(null);
      const current = trace.current_context_sync();
      expect(current.distinct_id).toBe('internal-id');
      expect(current.metadata).toEqual({ phase: 'internal', keep: 7 });
      return value;
    }, 7, { $baml: { trace: trace.hidden_sync().context_sync({
      distinct_id: 'root-id', metadata: { phase: 'root', keep: 7, remove: 9 },
    }) } })).toBe(7);
    expect(trace.current_cancel_token()).toBe(null);
    expect(trace.current_context_sync().metadata).toEqual({});
  });

  it('current_context_returns_detached_snapshot', async () => {
    expect(await baml.call_int_callback(value => {
      const observed = trace.current_context_sync();
      observed.metadata.request = 99;
      observed.distinct_id = 'changed';
      expect(trace.current_context_sync().metadata).toEqual({ request: 7 });
      expect(trace.current_context_sync().distinct_id).toBe('original');
      return value;
    }, 1, { $baml: { trace: trace.context_sync({ distinct_id: 'original', metadata: { request: 7 } }) } })).toBe(1);
  });

  it('concurrent_invocations_isolate_context', async () => {
    let markReady!: () => void;
    const ready = new Promise<void>(resolve => { markReady = resolve; });
    let unblock!: () => void;
    const release = new Promise<void>(resolve => { unblock = resolve; });
    let entered = 0;
    const pending = [0, 1].map(index => {
      const callback = withExecutionContext(async (active, value: number) => {
        expect(trace.current_context_sync().metadata).toEqual({ request: index });
        if (++entered === 2) markReady();
        await release;
        expect(active.run(() => trace.current_context_sync().metadata)).toEqual({ request: index });
        const nested = await active.run(() => contextBaml.current_context());
        expect(nested.metadata).toEqual({ request: index });
        return value;
      });
      return baml.call_int_callback(callback as unknown as (value: number) => number, index,
        { $baml: { trace: trace.hidden_sync().context_sync({ metadata: { request: index } }) } });
    });
    try { await ready; } finally { unblock(); }
    expect(await Promise.all(pending)).toEqual([0, 1]);
    expect(trace.current_cancel_token()).toBe(null);
    expect(trace.current_context_sync().metadata).toEqual({});
  });

  it('retained_context_survives_parent_completion', async () => {
    let captured: ReturnType<typeof currentExecutionContext> = null;
    expect(await baml.call_int_callback(value => {
      captured = currentExecutionContext();
      return value;
    }, 1, { $baml: { trace: trace.hidden_sync().context_sync({ metadata: { request: 7 } }) } })).toBe(1);
    expect(trace.current_cancel_token()).toBe(null);
    const active = captured!;
    expect(active.run(() => trace.current_context_sync().metadata)).toEqual({ request: 7 });
    expect(await active.run(() => baml.call_int_callback(value => {
      expect(trace.current_context_sync().metadata).toEqual({ request: 7 });
      return value;
    }, 7))).toBe(7);
    expect(trace.current_cancel_token()).toBe(null);
    expect(trace.current_context_sync().metadata).toEqual({});
  });

  it('reservation_does_not_inherit', async () => {
    const reserved = trace.span_sync().context_sync({ metadata: { request: 7 } }).reserve_sync();
    const callback = withExecutionContext(async (active, value: number) => {
      expect((await active.run(() => contextBaml.current_context())).metadata).toEqual({ request: 7 });
      return value;
    });
    expect(await baml.call_int_callback(callback as unknown as (value: number) => number, 1,
      { $baml: { trace: reserved } })).toBe(1);
    await expect(contextBaml.current_context({ $baml: { trace: reserved } }))
      .rejects.toThrow(/invalid trace reservation: AlreadyAttached/);
  });

  it('hidden_mode_does_not_inherit', async () => {
    expect(await contextBaml.child_span_under_hidden_parent({ $baml: { trace: trace.hidden_sync() } })).toBe(true);
  });

  it('cancelled_waiter_preserves_callback_context', async () => {
    const source = CancelToken.new_sync();
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
    const callback = withExecutionContext(async (active, value: number) => {
      markStarted();
      await new Promise<void>(resolve => active.signal.aborted ? resolve()
        : active.signal.addEventListener('abort', () => resolve(), { once: true }));
      markCleanup();
      try {
        expect(active.run(() => trace.current_context_sync().metadata)).toEqual({ request: 7 });
        expect(active.run(() => trace.current_cancel_token()!.is_cancelled_sync())).toBe(true);
        await release;
        expect(active.run(() => trace.current_context_sync().metadata)).toEqual({ request: 7 });
      } catch (error) {
        // The cancelled waiter cannot report failures from physical cleanup.
        cleanupErrors.push(error);
        throw error;
      } finally { exited = true; markExited(); }
      return value;
    });
    const pending = baml.call_int_callback(callback as unknown as (value: number) => number, 1,
      { $baml: { cancel: source, trace: trace.hidden_sync().context_sync({ metadata: { request: 7 } }) } });
    try {
      await started; source.cancel_sync();
      await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
      await cleanup;
      expect(exited).toBe(false);
      expect(trace.current_cancel_token()).toBe(null);
      expect(trace.current_context_sync().metadata).toEqual({});
    } finally { unblock(); }
    await finished;
    expect(cleanupErrors).toEqual([]);
  });
});
