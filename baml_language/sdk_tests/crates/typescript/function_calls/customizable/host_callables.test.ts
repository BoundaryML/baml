import "./baml_sdk/index.js";
import { BamlAbortError, BamlCallContext, BamlError, BamlPanic, initializeRuntimeFromBytecode } from "@boundaryml/baml-bridge";
import { describe, expect, it } from "vitest";
import { BYTECODE } from "./baml_sdk/_inlinedbaml.js";
import {
  Person,
  ValidationError,
  call_callback_with_optional_args_all_set_async,
  call_callback_with_optional_args_all_unset_async,
  call_callback_with_optional_args_partially_set_async,
  call_int_callback_async,
  call_repeatedly_async,
  call_returned_callback_async,
  call_returned_callback_in_list_async,
  call_with_callback,
  call_with_callback_async,
  call_with_class_callback_async,
  call_with_throwing_async,
  call_with_throwing_propagating_async,
  call_with_typed_throws_async,
  call_with_typed_throws_propagating_async,
  call_with_two_args_async,
  make_adder,
  make_counter,
  make_pair_builder,
} from "./baml_sdk/host_callable_tests/index.js";
import { isTestRuntime } from "./test_runtime.js";

describe("function_calls — generated SDK host callables", () => {
  it("host_callables_simple_sync_callable_returns_string", async () => {
    const cb = (x: number) => `got ${x}`;

    await expect(call_with_callback_async(cb, 5)).resolves.toBe("got 5");
  });

  it("host_callables_two_arg_callable_unpacks_positional_args", async () => {
    const cb = (x: number, prefix: string) => `${prefix}:${x}`;

    await expect(call_with_two_args_async(cb, 7, "answer")).resolves.toBe(
      "answer:7",
    );
  });

  it("host_callables_int_return_callable_round_trip", async () => {
    const cb = (x: number) => x * 2;

    await expect(call_int_callback_async(cb, 21)).resolves.toBe(42);
  });

  it("baml_closure_is_a_native_callable_with_host_language_arguments", () => {
    const addTen = make_adder(10);
    expect(typeof addTen).toBe("function");
    expect(addTen(5)).toBe(15);
    expect(addTen(7)).toBe(17);
  });

  it("baml_closure_decodes_multiple_args_and_structured_return_values", () => {
    const build = make_pair_builder(30);
    expect(build(12, "Ada")).toEqual(new Person({ name: "Ada", age: 42 }));
    expect(build(5, "Grace")).toEqual(new Person({ name: "Grace", age: 35 }));
  });

  it("baml_closure_is_reusable_and_retains_mutable_captures", () => {
    const nextValue = make_counter(40);
    expect(nextValue()).toBe(41);
    expect(nextValue()).toBe(42);
  });

  it("host_callables_surfaces_a_throwing_callback_as_a_baml_error", async () => {
    const cb = (_x: number): string => {
      throw new Error("nope");
    };

    await expect(call_with_callback_async(cb, 1)).rejects.toThrow(/nope|Error/);
  });

  it("host_callables_preserves_same_realm_thrown_object_identity", async () => {
    const thrown = new Error("same object");
    const callback = (): string => { throw thrown; };
    await expect(call_with_throwing_propagating_async(callback, 1)).rejects.toBe(thrown);
  });

  it("host_callables_preserves_arbitrary_thrown_js_values_without_hanging", async () => {
    const values: unknown[] = [
      new Error("ordinary error"),
      "string failure",
      73,
      null,
      { reason: "plain object" },
    ];

    for (const thrown of values) {
      const callback = (): string => { throw thrown; };
      try {
        await call_with_throwing_propagating_async(callback, 1);
        expect.unreachable("the throwing callback unexpectedly resolved");
      } catch (caught) {
        expect(caught).toBe(thrown);
      }
    }
  });

  it("host_callables_preserves_an_error_whose_stack_is_not_a_string", async () => {
    const thrown = new Error("non-string stack");
    Object.defineProperty(thrown, "stack", { value: { frames: ["host"] } });
    const callback = (): string => { throw thrown; };

    await expect(call_with_throwing_propagating_async(callback, 1)).rejects.toBe(thrown);
  });

  it("host_callables_completes_through_the_metadata_fallback_for_a_hostile_thrown_object", async () => {
    const thrown = new Proxy({}, {
      get(_target, property) {
        if (property === "constructor" || property === "toString") throw new Error("hostile getter");
        return undefined;
      },
    });
    const callback = (): string => { throw thrown; };

    await expect(call_with_throwing_propagating_async(callback, 1)).rejects.toBeInstanceOf(BamlError);
  });

  it("host_callables_preserves_a_rejected_promise_reason_by_identity", async () => {
    const thrown = { reason: "rejected Promise" };
    const callback = ((_value: number) => Promise.reject(thrown)) as unknown as (value: number) => string;

    await expect(call_with_throwing_propagating_async(callback, 1)).rejects.toBe(thrown);
  });

  it("host_callables_round_trips_a_typed_baml_error_through_typed_catch_and_propagation", async () => {
    const typedValue = new ValidationError({ code: 422, message: "invalid profile", fields: ["name"] });
    const callback = (): string => { throw new BamlError("validation failed", { value: typedValue }); };

    await expect(call_with_typed_throws_async(callback, 1)).resolves.toBe("caught: invalid profile");
    try {
      await call_with_typed_throws_propagating_async(callback, 1);
      expect.unreachable("the typed throw unexpectedly resolved");
    } catch (caught) {
      expect(caught).toBeInstanceOf(BamlError);
      expect((caught as BamlError).value).toBeInstanceOf(ValidationError);
      expect((caught as BamlError).value).toEqual(typedValue);
    }
  });

  it("host_callables_surfaces_a_wrong_callback_return_type_as_host_contract_violation", async () => {
    const callback = ((_value: number) => "not an int") as unknown as (value: number) => number;

    try {
      await call_int_callback_async(callback, 1);
      expect.unreachable("the wrong-type callback unexpectedly resolved");
    } catch (caught) {
      expect(caught).toBeInstanceOf(BamlPanic);
      expect((caught as BamlPanic).className).toBe("baml.panics.HostContractViolation");
    }
  });

  it("host_callables_adopts_a_custom_thenable_exactly_once", async () => {
    let settlements = 0;
    const callback = (value: number): string => ({
      then(resolve: (result: string) => void, reject: (reason: unknown) => void) {
        settlements += 1;
        resolve(`thenable-${value}`);
        reject(new Error("late rejection"));
      },
    }) as unknown as string;
    await expect(call_with_callback_async(callback, 8)).resolves.toBe("thenable-8");
    expect(settlements).toBe(1);
  });

  it.runIf(isTestRuntime("web"))("host_callables_adopts_a_promise_from_a_separate_browser_realm", async () => {
    type ForeignIframe = { contentWindow: { Promise: PromiseConstructor } | null; remove(): void };
    type BrowserDocument = { createElement(name: "iframe"): ForeignIframe; body: { append(value: ForeignIframe): void } };
    const browserDocument = (globalThis as unknown as { document: BrowserDocument }).document;
    const iframe = browserDocument.createElement("iframe");
    browserDocument.body.append(iframe);
    try {
      const ForeignPromise = iframe.contentWindow?.Promise;
      if (ForeignPromise === undefined) throw new Error("iframe Promise realm unavailable");
      const callback = ((value: number) => new ForeignPromise<string>((resolve: (result: string) => void) => resolve(`foreign-${value}`))) as unknown as (value: number) => string;
      await expect(call_with_callback_async(callback, 9)).resolves.toBe("foreign-9");
    } finally {
      iframe.remove();
    }
  });

  it("host_callables_returns_and_invokes_a_nested_host_callable", async () => {
    const factory = () => (value: number) => `nested-${value}`;
    await expect(call_returned_callback_async(factory, 6)).resolves.toBe("nested-6");
  });

  it("host_callables_returns_and_invokes_a_host_callable_nested_in_a_list", async () => {
    const factory = () => [(value: number) => `nested-list-${value}`];
    await expect(call_returned_callback_in_list_async(factory, 7)).resolves.toBe("nested-list-7");
  });

  it("host_callables_completes_a_pending_host_call_after_runtime_replacement", async () => {
    let resolveResult!: (value: string) => void;
    let dispatched!: () => void;
    const wasDispatched = new Promise<void>((resolve) => { dispatched = resolve; });
    const callback = ((_value: number) => new Promise<string>((resolve) => {
      resolveResult = resolve;
      dispatched();
    })) as unknown as (value: number) => string;

    const pending = call_with_callback_async(callback, 9);
    await wasDispatched;
    initializeRuntimeFromBytecode(BYTECODE);
    resolveResult("after-replacement");
    await expect(pending).resolves.toBe("after-replacement");
  });

  it("host_callables_ignores_a_host_promise_settlement_after_its_outer_call_is_cancelled", async () => {
    const ctx = new BamlCallContext();
    let settle!: (value: string) => void;
    let dispatched!: () => void;
    const wasDispatched = new Promise<void>((resolve) => { dispatched = resolve; });
    const callback = ((_value: number) => new Promise<string>((resolve) => {
      settle = resolve;
      dispatched();
    })) as unknown as (value: number) => string;

    const pending = call_with_callback_async(callback, 10, { $ctx: ctx });
    await wasDispatched;
    ctx.abort();
    await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
    settle("too late");
    await new Promise((resolve) => setTimeout(resolve, 0));
    await expect(call_with_callback_async((value) => `later-${value}`, 11)).resolves.toBe("later-11");
  });

  it("host_callables_cancels_through_the_originating_runtime_after_runtime_replacement", async () => {
    const ctx = new BamlCallContext();
    let settle!: (value: string) => void;
    let dispatched!: () => void;
    const wasDispatched = new Promise<void>((resolve) => { dispatched = resolve; });
    const callback = ((_value: number) => new Promise<string>((resolve) => {
      settle = resolve;
      dispatched();
    })) as unknown as (value: number) => string;

    const pending = call_with_callback_async(callback, 12, { $ctx: ctx });
    await wasDispatched;
    initializeRuntimeFromBytecode(BYTECODE);
    ctx.abort();
    await expect(pending).rejects.toBeInstanceOf(BamlAbortError);
    settle("late after replacement");
  });

  // FinalizationRegistry scheduling is nondeterministic and the runners do not expose forced GC; deterministic registry release is covered by the raw Web bridge tests.
  // SDK_PARITY_LINT(skip): callable release coverage depends on host weak-reference support and remains nondeterministic
  it.skip("host_callables_release_fires_on_drop_of_callable", async () => {
    async function callAndDrop(): Promise<WeakRef<object>> {
      let cb: ((x: number) => string) | undefined = (x: number) => `${x}`;
      const ref = new WeakRef(cb);

      await expect(call_with_callback_async(cb, 3)).resolves.toBe("3");
      cb = undefined;
      return ref;
    }

    const ref = await callAndDrop();
    expect(ref.deref()).toBeUndefined();
  });

  it("host_callables_round_trips_an_arrow_function_callback", async () => {
    await expect(
      call_with_callback_async((x: number) => `lambda-${x}`, 99),
    ).resolves.toBe("lambda-99");
  });

  it("host_callables_awaits_a_promise_returning_callback", async () => {
    const cb = async (x: number): Promise<string> => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      return `async-${x}`;
    };

    await expect(
      call_with_callback_async(cb as unknown as (arg0: number) => string, 4),
    ).resolves.toBe("async-4");
  });

  it("host_callables_multiple_callable_keys_are_distinct", async () => {
    const counter = { a: 0, b: 0 };
    const cbA = (x: number) => {
      counter.a += 1;
      return `a:${x}`;
    };
    const cbB = (x: number) => {
      counter.b += 1;
      return `b:${x}`;
    };

    await expect(call_with_callback_async(cbA, 1)).resolves.toBe("a:1");
    await expect(call_with_callback_async(cbB, 2)).resolves.toBe("b:2");
    expect(counter).toEqual({ a: 1, b: 1 });
  });

  it("host_callables_passes_a_generated_class_instance_into_the_callback", async () => {
    const cb = (p: Person) => {
      expect(p).toBeInstanceOf(Person);
      return `${p.name} is ${p.age}`;
    };
    const person = new Person({ name: "Ada", age: 37 });

    await expect(call_with_class_callback_async(cb, person)).resolves.toBe(
      "Ada is 37",
    );
  });

  it("host_callables_call_repeatedly_invokes_callback_n_times", async () => {
    const invocations: number[] = [];
    const cb = (x: number) => {
      invocations.push(x);
      return `item-${x}`;
    };

    await expect(call_repeatedly_async(cb, 5)).resolves.toEqual([
      "item-0",
      "item-1",
      "item-2",
      "item-3",
      "item-4",
    ]);
    expect(invocations).toEqual([0, 1, 2, 3, 4]);
  });

  it("host_callables_call_repeatedly_with_zero_n_returns_empty_list", async () => {
    const invocations: number[] = [];
    const cb = (x: number) => {
      invocations.push(x);
      return `${x}`;
    };

    await expect(call_repeatedly_async(cb, 0)).resolves.toEqual([]);
    expect(invocations).toEqual([]);
  });

  it("host_callables_call_with_throwing_in_baml_catches_host_callable_error", async () => {
    // The fixture's body is `callback(x) catch (e) { _ => "caught:" + e.class_name }`.
    // Now that sysop throws are injected into the VM's exception unwinder, the
    // BAML `catch` actually fires and the function resolves to the caught
    // string — instead of the pre-fix behaviour where the throw escaped the
    // catch and surfaced to the host as a `baml.errors.HostCallable` reject.
    const cb = (_x: number): string => {
      throw new Error("boom from host");
    };

    await expect(call_with_throwing_async(cb, 1)).resolves.toBe("caught:Error");
  });
});

describe("function_calls — generated SDK sync guard for host callables", () => {
  it("host_callables_rejects_callable_args_on_the_generated_sync_path_instead_of_hanging", { timeout: 2_000 }, () => {
    expect(() => call_with_callback((x: number) => `got ${x}`, 5)).toThrow(
      /host callable/i,
    );
  });
});

// Optional args × host callables (the combination): a host callable whose
// own type carries optional parameters (`(x: int, y?: int, z?: int) -> int`).
// Defaults aren't allowed inside a callable type — only the `?` optional marker
// — so the host's own default is the only source of a value when BAML omits the
// arg. `y` and `z` cross the boundary by name, so each can be supplied or
// omitted independently.
describe("function_calls — optional-arg host callables (the combination)", () => {
  // The callback type codegens with the optional args grouped into a trailing
  // `$opts` object — `(x: number, $opts?: { y?: number; z?: number }) => number`
  // — mirroring the convention for *calling* a BAML function. The engine
  // dispatches positionally + by-name; the bridge's dispatch decoder
  // (`makeHostCallableDispatch` in `proto.ts`) folds the supplied optionals back
  // into the `$opts` object, so the callback below reads `$opts`. It returns
  // `x*100 + y*10 + z` so each test can read off which optionals were delivered
  // (omitted ones fall back to the callback's own `?? 8` / `?? 9`).
  const cb = (x: number, $opts?: { y?: number; z?: number }) =>
    x * 100 + ($opts?.y ?? 8) * 10 + ($opts?.z ?? 9);

  it("host_callables_omits_both_optionals_so_the_callback_s_own_defaults_apply", async () => {
    // `callback(x)` supplies neither optional; both are dropped before dispatch,
    // so the callback runs with no `$opts` object and its own `?? 8` / `?? 9`
    // fill them, yielding `5*100 + 8*10 + 9 = 589`.
    await expect(
      call_callback_with_optional_args_all_unset_async(cb, 5),
    ).resolves.toEqual([589]);
  });

  it("host_callables_delivers_a_single_supplied_optional_by_name_defaulting_the_rest", async () => {
    // Two calls each supplying exactly one optional: `callback(x, y = 2)`
    // (→ 500 + 20 + 9 = 529) then `callback(x, z = 3)` (→ 500 + 80 + 3 = 583).
    // Optionals cross by name, so each supplied value lands in `$opts` and the
    // omitted one falls back to the default — including the case where the
    // leading `y` is skipped while `z` is supplied.
    await expect(
      call_callback_with_optional_args_partially_set_async(cb, 5),
    ).resolves.toEqual([529, 583]);
  });

  it("host_callables_delivers_both_supplied_optionals_in_one_opts_object", async () => {
    // `callback(x, y = 2, z = 3)` supplies both optionals; both arrive in `$opts`
    // and override the callback's defaults.
    await expect(
      call_callback_with_optional_args_all_set_async(cb, 5),
    ).resolves.toEqual([523]);
  });
});
