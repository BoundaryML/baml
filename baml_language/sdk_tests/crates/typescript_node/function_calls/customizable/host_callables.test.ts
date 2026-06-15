import "./baml_sdk/index.js";
import { BamlError } from "@boundaryml/baml-core-node";
import { describe, expect, it } from "vitest";
import { AccessError } from "./baml_sdk/baml/errors/index.js";
import {
  Person,
  call_int_callback_async,
  call_optional_callback_omitted_async,
  call_optional_callback_supplied_async,
  call_optional_int_callback_supplied_async,
  call_repeatedly_async,
  call_with_callback,
  call_with_callback_async,
  call_with_class_callback_async,
  call_with_throwing_async,
  call_with_two_args_async,
} from "./baml_sdk/host_callable_tests/index.js";

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

    await expect(call_with_callback_async(cb, 1)).rejects.toThrow(
      /nope|Error/,
    );
  });

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
      await new Promise<void>((resolve) => setImmediate(resolve));
      return `async-${x}`;
    };

    await expect(
      call_with_callback_async(
        cb as unknown as (arg0: number) => string,
        4,
      ),
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
  it("rejects callable args on the generated sync path instead of hanging", () => {
    expect(() => call_with_callback((x: number) => `got ${x}`, 5)).toThrow(
      /host callable/i,
    );
  });
});

// Optional args × host callables (the combination): a host callable whose
// own type carries an optional parameter (`(x: int, y?: int) -> ...`).
// Optional args (optional_args.test.ts) and host callables (above) were each
// covered alone but never together. Defaults aren't allowed inside a callable
// type — only the `?` optional marker — so the host's own default is the only
// possible source of a value when BAML omits the arg.
describe("function_calls — optional-arg host callables (the combination)", () => {
  // NOTE: a named/optional callable type (`(x: int, y?: int) -> T`) codegens
  // to the loose `(...args: unknown[]) => T` signature in TS — unlike an
  // unnamed positional type (`(int) -> string`), which yields a precise
  // `(arg0: number) => string`. The callbacks below take `...args: unknown[]`
  // to match that generated contract.
  it("passes a supplied optional arg through positionally", async () => {
    // BAML resolves `callback(x, y = 9)` into the callable's positional arg
    // list, so the host receives both values positionally — overriding its
    // own default.
    const seen: unknown[][] = [];
    const cb = (...args: unknown[]) => {
      seen.push(args);
      const [x, y] = args;
      return `${x}:${y}`;
    };

    await expect(call_optional_callback_supplied_async(cb, 5)).resolves.toBe(
      "5:9",
    );
    expect(seen).toEqual([[5, 9]]);
  });

  it("round-trips a supplied optional arg with an int return", async () => {
    const cb = (...args: unknown[]) =>
      (args[0] as number) * 10 + (args[1] as number);

    await expect(
      call_optional_int_callback_supplied_async(cb, 5),
    ).resolves.toBe(150);
  });

  it("rejects an omitted optional arg with an AccessError before dispatch", async () => {
    // Unlike a BAML→BAML call — where the engine resolves an omitted optional
    // natively — the omitted-arg sentinel cannot cross the host serialization
    // boundary, so the engine raises `baml.errors.AccessError` *before* the
    // callback ever runs. Pins the current limitation: a host callable's
    // optional arg cannot be omitted at the call site.
    const invoked: unknown[][] = [];
    const cb = (...args: unknown[]) => {
      invoked.push(args);
      const [x, y] = args;
      return `${x}:${y}`;
    };

    let caught: unknown;
    try {
      await call_optional_callback_omitted_async(cb, 5);
      expect.unreachable("expected an AccessError to be thrown");
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(BamlError);
    expect((caught as BamlError).value).toBeInstanceOf(AccessError);
    expect(invoked).toEqual([]); // the engine fails before dispatching
  });
});
