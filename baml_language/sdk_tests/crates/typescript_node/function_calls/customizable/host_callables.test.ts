import "./baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import {
  Person,
  call_callback_with_optional_args_all_set_async,
  call_callback_with_optional_args_all_unset_async,
  call_callback_with_optional_args_partially_set_async,
  call_int_callback_async,
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
