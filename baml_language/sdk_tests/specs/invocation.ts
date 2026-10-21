/** BEP-081 invocation audit spec for the function_calls fixture.
 * Place beside that fixture's generated baml_sdk when the API exists.
 * No runner, mock runtime, dispatch helpers, instrumentation, or flush.
 * Shared names/groups match invocation.py; language-only names use a suffix.
 * Existing callable suites cover execution patterns. Use an external process
 * deadline for blocking cases. See invocation.py's boundary-only requirements:
 * those need raw ABI tests, not invented public observation APIs.
 * No promised stream producer cancellation or defaults/hooks visibility.
 */
import assert from "node:assert/strict";
import { BamlAbortError, BamlCancelledError } from "@boundaryml/baml-bridge";
import {
  type BamlOptions, OptBox, hello_world, hello_world_async,
  optional_args_probe, optional_args_probe_async, trace,
} from "./baml_sdk/index.js";
import { invoke } from "./baml_sdk/experimental.js";
// Private carrier access is for execution-context conformance only.
import { _currentExecutionContext as currentExecutionContext, _withExecutionContext as withExecutionContext } from "@boundaryml/baml-bridge";
import * as baml from "./baml_sdk/host_callable_tests/index.js";
import { CancelToken } from "./baml_sdk/baml/spawn/index.js";

// invocation_options
export function four_call_forms() {
  const opts: BamlOptions = { timeoutMs: 1000 };
  assert.deepEqual(optional_args_probe(1), [1, 5, 99]);
  assert.deepEqual(optional_args_probe(1, { opt1: 7 }), [1, 7, 99]);
  assert.deepEqual(optional_args_probe(1, { $baml: opts }), [1, 5, 99]);
  assert.deepEqual(optional_args_probe(1, { opt1: 7, $baml: opts }), [1, 7, 99]);
}

export async function four_call_forms_async() {
  const opts: BamlOptions = { timeoutMs: 1000 };
  assert.deepEqual(await optional_args_probe_async(1), [1, 5, 99]);
  assert.deepEqual(await optional_args_probe_async(1, { opt1: 7 }), [1, 7, 99]);
  assert.deepEqual(await optional_args_probe_async(1, { $baml: opts }), [1, 5, 99]);
  assert.deepEqual(await optional_args_probe_async(1, { opt1: 7, $baml: opts }), [1, 7, 99]);
}

export function empty_controls() {
  assert.equal(hello_world(), "hello world");
  assert.equal(hello_world({ $baml: {} }), "hello world");
  assert.equal(hello_world({ $baml: null }), "hello world");
  assert.equal(hello_world({ $baml: { trace: null, cancel: null, timeoutMs: null } }), "hello world");
}

export function omitted_argument_is_not_null() {
  assert.deepEqual(optional_args_probe(1, { $baml: {} }), [1, 5, 99]);
  assert.deepEqual(optional_args_probe(1, { opt1: null, $baml: {} }), [1, null, 99]);
}

export function unknown_control_rejected() {
  // Deliberately bypass static checking to exercise runtime validation.
  const opts = { unknown: true } as unknown as BamlOptions;
  assert.throws(() => hello_world({ $baml: opts }));
  // Must be a preparation usage error, not target execution failure.
}

export function invalid_timeout_rejected() {
  for (const timeoutMs of [-1, 0.5, Infinity, NaN, 2147483648]) {
    assert.throws(() => hello_world({ $baml: { timeoutMs } }));
  }
}

export function timeout_upper_bound_accepted() {
  assert.equal(hello_world({ $baml: { timeoutMs: 2147483647 } }), "hello world");
}

// invocation_surfaces
export function methods_accept_controls() {
  const box = OptBox.make(1, { $baml: {} });
  assert.equal(box.base, 8);
  assert.deepEqual(box.probe(2, { $baml: {} }), [8, 2, 5]);
  assert.deepEqual(box.probe(2, { opt1: null, $baml: {} }), [8, 2, null]);
}

export async function methods_accept_controls_async() {
  const box = await OptBox.make_async(1, { $baml: {} });
  assert.equal(box.base, 8);
  assert.deepEqual(await box.probe_async(2, { $baml: {} }), [8, 2, 5]);
}

export async function returned_callable_accepts_controls() {
  const add = baml.make_adder(3);
  assert.equal(add(4, { $baml: {} }), 7);
  assert.equal(await add.callAsync(4, { $baml: {} }), 7);
}

export function dynamic_call_accepts_controls() {
  assert.deepEqual(invoke("user.optional_args_probe", { arg0: 1 }, { $baml: {} }), [1, 5, 99]);
  assert.deepEqual(invoke("user.optional_args_probe", { arg0: 1, opt1: null }, { $baml: {} }), [1, null, 99]);
  // Dynamic application maps never strip $baml as a control.
  // Generic $types is separate; specialized handles reject new type bindings.
}

// invocation_lifecycle
export async function pre_cancelled_call_does_not_enter_callback() {
  const token = CancelToken.new();
  token.cancel();
  const calls: number[] = [];
  const callback = (value: number) => { calls.push(value); return value; };
  await assert.rejects(
    baml.call_int_callback_async(callback, 1, { $baml: { cancel: token } }),
    (error: unknown) => error instanceof BamlAbortError && error.reason instanceof BamlCancelledError,
  );
  assert.deepEqual(calls, []);
}

export async function zero_timeout_does_not_enter_callback() {
  const calls: number[] = [];
  const callback = (value: number) => { calls.push(value); return value; };
  await assert.rejects(
    baml.call_int_callback_async(callback, 1, { $baml: { timeoutMs: 0 } }),
    (error: unknown) => error instanceof BamlAbortError && error.reason instanceof BamlCancelledError,
  );
  assert.deepEqual(calls, []);
}

export async function composite_token_observes_every_source() {
  for (const index of [0, 1]) {
    const sources = [CancelToken.new(), CancelToken.new()];
    const combined = CancelToken.any(sources);
    sources[index]!.cancel();
    assert.equal(combined.is_cancelled(), true);
    await assert.rejects(
      hello_world_async({ $baml: { cancel: combined } }),
      (error: unknown) => error instanceof BamlAbortError && error.reason instanceof BamlCancelledError,
    );
    assert.equal(sources[1 - index]!.is_cancelled(), false);
  }
}

export async function child_cancellation_does_not_cancel_input() {
  const upstream = CancelToken.new();
  const callback = (value: number) => {
    const active = currentExecutionContext();
    assert.ok(active);
    active.run(() => trace.current_cancel_token()!).cancel();
    // Exact token operations remain serviceable under ambient cancellation.
    assert.equal(active.run(() => trace.current_cancel_token()!).is_cancelled(), true);
    assert.equal(upstream.is_cancelled(), false);
    return value;
  };
  try {
    await baml.call_int_callback_async(callback, 1, { $baml: { cancel: upstream } });
  } catch (error) {
    assert.ok(error instanceof BamlAbortError && error.reason instanceof BamlCancelledError);
  }
  // Callback success may win the completion race; either outcome is valid.
  assert.equal(upstream.is_cancelled(), false);
  assert.equal(await hello_world_async({ $baml: { cancel: upstream } }), "hello world");
}

export async function rejected_admission_does_not_consume_reservation() {
  const reserved = trace.span().reserve();
  const token = CancelToken.new();
  token.cancel();
  await assert.rejects(
    hello_world_async({ $baml: { trace: reserved, cancel: token } }),
    (error: unknown) => error instanceof BamlAbortError && error.reason instanceof BamlCancelledError,
  );
  assert.equal(hello_world({ $baml: { trace: reserved } }), "hello world");
}

export async function concurrent_reservation_attaches_once() {
  const reserved = trace.span().reserve();
  const results = await Promise.allSettled([
    hello_world_async({ $baml: { trace: reserved } }),
    hello_world_async({ $baml: { trace: reserved } }),
  ]);
  assert.equal(results.filter(result => result.status === "fulfilled" && result.value === "hello world").length, 1);
  assert.equal(results.filter(result => result.status === "rejected").length, 1);
  // Losing call must have the documented attachment failure, never enter body.
}

// invocation_callbacks
export async function callback_frame_and_reentry() {
  assert.equal(trace.current_cancel_token(), null);
  const upstream = CancelToken.new();
  const frames: NonNullable<ReturnType<typeof currentExecutionContext>>[] = [];
  const leaf = async (value: number) => {
    const active = currentExecutionContext();
    assert.ok(active);
    assert.equal(active.run(() => trace.current_cancel_token()!).is_cancelled(), false);
    // Re-entry has a fresh execution identity; wrapper identity is unspecified.
    return value + 1;
  };
  const callback = async (value: number) => {
    const active = currentExecutionContext();
    assert.ok(active);
    frames.push(active);
    await Promise.resolve();
    assert.ok(trace.current_cancel_token());
    assert.equal(trace.current_cancel_token()!.is_cancelled(), false);
    return await baml.call_int_callback_async(leaf, value, { $baml: {} });
  };
  assert.equal(await baml.call_int_callback_async(callback, 6, { $baml: { cancel: upstream } }), 7);
  assert.equal(trace.current_cancel_token(), null);
  assert.equal(upstream.is_cancelled(), false);
  // Node ambient carrier; Web must port this through the private execution carrier.
}

export async function null_controls_preserve_inherited_context() {
  const callback = async (value: number) => await baml.call_int_callback_async(
    () => trace.current_context().metadata.request as number,
    value,
    { $baml: { trace: null, cancel: null, timeoutMs: null } },
  );
  assert.equal(await baml.call_int_callback_async(callback, 1, {
    $baml: { trace: trace.hidden().context({ metadata: { request: 7 } }) },
  }), 7);
  // Context survives even with recording disabled; no extra plumbing span.
}

export async function configuration_snapshot_is_not_live() {
  const opts: { trace: trace.Options } = {
    trace: trace.context({ metadata: { request: 7 } }),
  };
  const callback = async (value: number) => {
    opts.trace = trace.context({ metadata: { request: 99 } });
    await Promise.resolve();
    assert.equal(trace.current_context().metadata.request, 7);
    return value;
  };
  assert.equal(await baml.call_int_callback_async(callback, 1, { $baml: opts }), 1);
  assert.equal(opts.trace.inspect().context.metadata.request, 99);
}

export async function live_token_cancels_after_admission() {
  const source = CancelToken.new();
  const combined = CancelToken.any([source]);
  let markStarted!: () => void;
  let markExited!: () => void;
  let releaseCleanup!: () => void;
  let didExit = false;
  const started = new Promise<void>(resolve => { markStarted = resolve; });
  const exited = new Promise<void>(resolve => { markExited = resolve; });
  const cleanupGate = new Promise<void>(resolve => { releaseCleanup = resolve; });
  const callback = async (value: number) => {
    const active = currentExecutionContext();
    assert.ok(active);
    markStarted();
    try {
      await new Promise<void>(resolve => {
        if (active.signal.aborted) resolve();
        else active.signal.addEventListener("abort", () => resolve(), { once: true });
      });
    } finally {
      await cleanupGate;
      didExit = true;
      markExited();
    }
    return value;
  };
  const pending = baml.call_int_callback_async(callback, 1, { $baml: { cancel: combined } });
  // Register rejection handling immediately, before asking for cancellation.
  const completion = assert.rejects(pending,
    (error: unknown) => error instanceof BamlAbortError && error.reason instanceof BamlCancelledError);
  await started;
  source.cancel();
  await completion;
  assert.equal(didExit, false);
  releaseCleanup();
  await exited;
  // Waiter cancellation precedes actual host exit; cleanup still has an owner.
}

export async function retained_effective_token_stays_live_after_callback() {
  const source = CancelToken.new();
  const retained: CancelToken[] = [];
  const callback = (value: number) => {
    const active = currentExecutionContext();
    assert.ok(active);
    retained.push(active.run(() => trace.current_cancel_token()!));
    return value;
  };
  assert.equal(await baml.call_int_callback_async(callback, 1, { $baml: { cancel: source } }), 1);
  assert.equal(trace.current_cancel_token(), null);
  source.cancel();
  assert.equal(retained[0]!.is_cancelled(), true);
  // Keeping the token does not restore the frame or callback completion ID.
}

// invocation_callbacks_typescript_only
export async function native_signal_cancels_admission_typescript_only() {
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(
    hello_world_async({ $baml: { signal: controller.signal } }),
    (error: unknown) => error instanceof BamlAbortError && error.reason instanceof BamlCancelledError,
  );
}

export function sync_callback_entry_rejected_typescript_only() {
  const calls: number[] = [];
  assert.throws(() => baml.call_int_callback(
    (value: number) => { calls.push(value); return value; }, 1, { $baml: {} },
  ));
  assert.deepEqual(calls, []);
  // Unsupported execution; also reject callbacks hidden in returned handles.
}

export async function explicit_callback_carrier_typescript_only() {
  const callback = withExecutionContext(async (active, value: number) => {
    await Promise.resolve();
    return await active.run(() => hello_world_async({ $baml: {} })).then(() => value);
  });
  assert.equal(await baml.call_int_callback_async(callback, 7), 7);
  // Works on Web without relying on ambient state across Promise continuations.
}
