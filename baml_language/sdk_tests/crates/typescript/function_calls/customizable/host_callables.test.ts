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
} from "./baml_sdk/host_callable_tests/index.js";
import { isTestRuntime } from "./test_runtime.js";

describe("function_calls — generated SDK host callables", () => {
  it("passes a plain function callback and returns a string", async () => {
    const cb = (x: number) => `got ${x}`;

    await expect(call_with_callback_async(cb, 5)).resolves.toBe("got 5");
  });

  it("unpacks two callback args positionally", async () => {
    const cb = (x: number, prefix: string) => `${prefix}:${x}`;

    await expect(call_with_two_args_async(cb, 7, "answer")).resolves.toBe(
      "answer:7",
    );
  });

  it("round-trips an int callback return value", async () => {
    const cb = (x: number) => x * 2;

    await expect(call_int_callback_async(cb, 21)).resolves.toBe(42);
  });

  it("surfaces a throwing callback as a BAML error", async () => {
    const cb = (_x: number): string => {
      throw new Error("nope");
    };

    await expect(call_with_callback_async(cb, 1)).rejects.toThrow(/nope|Error/);
  });

  it("preserves same-realm thrown object identity", async () => {
    const thrown = new Error("same object");
    const callback = (): string => { throw thrown; };
    await expect(call_with_throwing_propagating_async(callback, 1)).rejects.toBe(thrown);
  });

  it("preserves arbitrary thrown JS values without hanging", async () => {
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

  it("preserves an Error whose stack is not a string", async () => {
    const thrown = new Error("non-string stack");
    Object.defineProperty(thrown, "stack", { value: { frames: ["host"] } });
    const callback = (): string => { throw thrown; };

    await expect(call_with_throwing_propagating_async(callback, 1)).rejects.toBe(thrown);
  });

  it("completes through the metadata fallback for a hostile thrown object", async () => {
    const thrown = new Proxy({}, {
      get(_target, property) {
        if (property === "constructor" || property === "toString") throw new Error("hostile getter");
        return undefined;
      },
    });
    const callback = (): string => { throw thrown; };

    await expect(call_with_throwing_propagating_async(callback, 1)).rejects.toBeInstanceOf(BamlError);
  });

  it("preserves a rejected Promise reason by identity", async () => {
    const thrown = { reason: "rejected Promise" };
    const callback = ((_value: number) => Promise.reject(thrown)) as unknown as (value: number) => string;

    await expect(call_with_throwing_propagating_async(callback, 1)).rejects.toBe(thrown);
  });

  it("round-trips a typed BamlError through typed catch and propagation", async () => {
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

  it("surfaces a wrong callback return type as HostContractViolation", async () => {
    const callback = ((_value: number) => "not an int") as unknown as (value: number) => number;

    try {
      await call_int_callback_async(callback, 1);
      expect.unreachable("the wrong-type callback unexpectedly resolved");
    } catch (caught) {
      expect(caught).toBeInstanceOf(BamlPanic);
      expect((caught as BamlPanic).className).toBe("baml.panics.HostContractViolation");
    }
  });

  it("adopts a custom thenable exactly once", async () => {
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

  it.runIf(isTestRuntime("web"))("adopts a Promise from a separate browser realm", async () => {
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

  it("returns and invokes a nested host callable", async () => {
    const factory = () => (value: number) => `nested-${value}`;
    await expect(call_returned_callback_async(factory, 6)).resolves.toBe("nested-6");
  });

  it("returns and invokes a host callable nested in a list", async () => {
    const factory = () => [(value: number) => `nested-list-${value}`];
    await expect(call_returned_callback_in_list_async(factory, 7)).resolves.toBe("nested-list-7");
  });

  it("completes a pending host call after runtime replacement", async () => {
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

  it("ignores a host Promise settlement after its outer call is cancelled", async () => {
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

  it("cancels through the originating runtime after runtime replacement", async () => {
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
  it.skip("releases callable objects after the engine drops the HostClosure", async () => {
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

  it("round-trips an arrow function callback", async () => {
    await expect(
      call_with_callback_async((x: number) => `lambda-${x}`, 99),
    ).resolves.toBe("lambda-99");
  });

  it("awaits a Promise-returning callback", async () => {
    const cb = async (x: number): Promise<string> => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
      return `async-${x}`;
    };

    await expect(
      call_with_callback_async(cb as unknown as (arg0: number) => string, 4),
    ).resolves.toBe("async-4");
  });

  it("keeps multiple callback registry keys distinct", async () => {
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

  it("passes a generated class instance into the callback", async () => {
    const cb = (p: Person) => {
      expect(p).toBeInstanceOf(Person);
      return `${p.name} is ${p.age}`;
    };
    const person = new Person({ name: "Ada", age: 37 });

    await expect(call_with_class_callback_async(cb, person)).resolves.toBe(
      "Ada is 37",
    );
  });

  it("invokes the callback once per BAML loop iteration", async () => {
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

  it("does not invoke the callback for a zero-iteration loop", async () => {
    const invocations: number[] = [];
    const cb = (x: number) => {
      invocations.push(x);
      return `${x}`;
    };

    await expect(call_repeatedly_async(cb, 0)).resolves.toEqual([]);
    expect(invocations).toEqual([]);
  });

  it("catches a host-callable throw in the BAML catch arm", async () => {
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
  it("rejects callable args on the generated sync path instead of hanging", { timeout: 2_000 }, () => {
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

  it("omits both optionals so the callback's own defaults apply", async () => {
    // `callback(x)` supplies neither optional; both are dropped before dispatch,
    // so the callback runs with no `$opts` object and its own `?? 8` / `?? 9`
    // fill them, yielding `5*100 + 8*10 + 9 = 589`.
    await expect(
      call_callback_with_optional_args_all_unset_async(cb, 5),
    ).resolves.toEqual([589]);
  });

  it("delivers a single supplied optional by name, defaulting the rest", async () => {
    // Two calls each supplying exactly one optional: `callback(x, y = 2)`
    // (→ 500 + 20 + 9 = 529) then `callback(x, z = 3)` (→ 500 + 80 + 3 = 583).
    // Optionals cross by name, so each supplied value lands in `$opts` and the
    // omitted one falls back to the default — including the case where the
    // leading `y` is skipped while `z` is supplied.
    await expect(
      call_callback_with_optional_args_partially_set_async(cb, 5),
    ).resolves.toEqual([529, 583]);
  });

  it("delivers both supplied optionals in one $opts object", async () => {
    // `callback(x, y = 2, z = 3)` supplies both optionals; both arrive in `$opts`
    // and override the callback's defaults.
    await expect(
      call_callback_with_optional_args_all_set_async(cb, 5),
    ).resolves.toEqual([523]);
  });
});
