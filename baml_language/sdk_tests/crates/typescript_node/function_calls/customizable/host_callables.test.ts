import "./baml_sdk/index.js";
import { describe, expect, it } from "vitest";
import { BamlError } from "@boundaryml/baml-bridge";
import {
  CallableFieldHolder,
  Person,
  ValidationError,
  call_callable_field_async,
  call_callback_with_optional_args_all_set_async,
  call_callback_with_optional_args_all_unset_async,
  call_callback_with_optional_args_partially_set_async,
  call_int_callback_async,
  call_repeatedly_async,
  call_with_callback_async,
  call_with_class_callback_async,
  call_with_throwing_async,
  call_with_typed_throws_async,
  call_with_typed_throws_propagating_async,
  call_with_two_args_async,
} from "./baml_sdk/host_callable_tests/index.js";

describe("function_calls — generated SDK host callables", () => {
  it("test_simple_sync_callable_returns_string", async () => {
    const cb = (x: number) => `got ${x}`;

    await expect(call_with_callback_async(cb, 5)).resolves.toBe("got 5");
  });

  it("test_two_arg_callable_unpacks_positional_args", async () => {
    const cb = (x: number, prefix: string) => `${prefix}:${x}`;

    await expect(call_with_two_args_async(cb, 7, "answer")).resolves.toBe(
      "answer:7",
    );
  });

  it("test_int_return_callable_round_trip", async () => {
    const cb = (x: number) => x * 2;

    await expect(call_int_callback_async(cb, 21)).resolves.toBe(42);
  });

  it("test_throwing_callable_round_trips_original_python_exception", async () => {
    const raised = new Error("nope");
    const cb = (_x: number): string => {
      throw raised;
    };

    try {
      await call_with_callback_async(cb, 1);
      throw new Error("expected callback error");
    } catch (error) {
      expect(error).toBe(raised);
    }
  });

  it("test_throwing_callable_keyerror_round_trips_with_identity", async () => {
    const raised = new TypeError("missing");
    const cb = (_x: number): string => {
      throw raised;
    };

    try {
      await call_with_callback_async(cb, 1);
      throw new Error("expected callback error");
    } catch (error) {
      expect(error).toBe(raised);
    }
  });

  it("test_throwing_callable_custom_python_exception_round_trips_with_identity", async () => {
    class MyDomainError extends Error {
      constructor(message: string, readonly code: number) {
        super(message);
      }
    }
    const raised = new MyDomainError("custom domain failure", 42);
    const cb = (_x: number): string => {
      throw raised;
    };

    try {
      await call_with_callback_async(cb, 1);
      throw new Error("expected callback error");
    } catch (error) {
      expect(error).toBe(raised);
      expect((error as MyDomainError).code).toBe(42);
    }
  });

  it("test_throwing_callable_bamlerror_wrapping_codegenned_class_is_caught_in_baml", async () => {
    const cb = (_x: number): string => {
      throw new BamlError("bad shape", {
        value: new ValidationError({
          code: 4,
          message: "bad shape",
          fields: ["name", "age", "email", "phone"],
        }),
      });
    };

    await expect(call_with_typed_throws_async(cb, 1)).resolves.toBe(
      "caught: bad shape",
    );
  });

  it("test_throwing_callable_bamlerror_propagates_back_with_typed_fields", async () => {
    const raised = new BamlError("propagated through", {
      value: new ValidationError({
        code: 7,
        message: "propagated through",
        fields: ["x", "y"],
      }),
    });
    const cb = (_x: number): string => {
      throw raised;
    };

    try {
      await call_with_typed_throws_propagating_async(cb, 1);
      throw new Error("expected typed BAML error");
    } catch (error) {
      expect(error).toBeInstanceOf(BamlError);
      const decoded = (error as BamlError).value;
      expect(decoded).toBeInstanceOf(ValidationError);
      expect(decoded).toMatchObject({
        code: 7,
        message: "propagated through",
        fields: ["x", "y"],
      });
    }
  });

  it("test_throwing_async_callable_round_trips_original_python_exception", async () => {
    const raised = new Error("async nope");
    const cb = async (_x: number): Promise<string> => {
      throw raised;
    };

    try {
      await call_with_callback_async(cb, 1);
      throw new Error("expected callback error");
    } catch (error) {
      expect(error).toBe(raised);
    }
  });

  it("test_multiple_throws_in_flight_do_not_collide_in_registry", async () => {
    const first = new Error("first");
    const second = new Error("second");
    const results = await Promise.allSettled([
      call_with_callback_async(() => {
        throw first;
      }, 1),
      call_with_callback_async(() => {
        throw second;
      }, 2),
    ]);

    expect(results[0]).toMatchObject({ status: "rejected", reason: first });
    expect(results[1]).toMatchObject({ status: "rejected", reason: second });
  });

  it.skip("test_release_fires_on_drop_of_callable", async () => {
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

  it("test_lambda_round_trip", async () => {
    await expect(
      call_with_callback_async((x: number) => `lambda-${x}`, 99),
    ).resolves.toBe("lambda-99");
  });

  it("test_async_callable_runs_to_completion", async () => {
    const cb = async (x: number): Promise<string> => {
      await new Promise<void>((resolve) => setImmediate(resolve));
      return `async-${x}`;
    };

    await expect(
      call_with_callback_async(cb, 4),
    ).resolves.toBe("async-4");
  });

  it("test_multiple_callable_keys_are_distinct", async () => {
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

  it("test_class_callback_round_trips_pydantic_model", async () => {
    const cb = (p: Person) => {
      expect(p).toBeInstanceOf(Person);
      return `${p.name} is ${p.age}`;
    };
    const person = new Person({ name: "Ada", age: 37 });

    await expect(call_with_class_callback_async(cb, person)).resolves.toBe(
      "Ada is 37",
    );
  });

  it("test_callable_valued_class_field_round_trips", async () => {
    const holder = new CallableFieldHolder({
      prefix: "value=",
      callback: (value: number) => String(value * 2),
    });

    await expect(call_callable_field_async(holder, 21)).resolves.toBe(
      "value=42",
    );
  });

  it("test_call_repeatedly_invokes_callback_n_times", async () => {
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

  it("test_call_repeatedly_with_zero_n_returns_empty_list", async () => {
    const invocations: number[] = [];
    const cb = (x: number) => {
      invocations.push(x);
      return `${x}`;
    };

    await expect(call_repeatedly_async(cb, 0)).resolves.toEqual([]);
    expect(invocations).toEqual([]);
  });

  it("test_call_with_throwing_in_baml_catches_host_callable_error", async () => {
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

  it("test_optional_args_all_unset_apply_host_defaults", async () => {
    // `callback(x)` supplies neither optional; both are dropped before dispatch,
    // so the callback runs with no `$opts` object and its own `?? 8` / `?? 9`
    // fill them, yielding `5*100 + 8*10 + 9 = 589`.
    await expect(
      call_callback_with_optional_args_all_unset_async(cb, 5),
    ).resolves.toEqual([589]);
  });

  it("test_optional_args_partially_set_deliver_by_name", async () => {
    // Two calls each supplying exactly one optional: `callback(x, y = 2)`
    // (→ 500 + 20 + 9 = 529) then `callback(x, z = 3)` (→ 500 + 80 + 3 = 583).
    // Optionals cross by name, so each supplied value lands in `$opts` and the
    // omitted one falls back to the default — including the case where the
    // leading `y` is skipped while `z` is supplied.
    await expect(
      call_callback_with_optional_args_partially_set_async(cb, 5),
    ).resolves.toEqual([529, 583]);
  });

  it("test_optional_args_all_set_deliver_both", async () => {
    // `callback(x, y = 2, z = 3)` supplies both optionals; both arrive in `$opts`
    // and override the callback's defaults.
    await expect(
      call_callback_with_optional_args_all_set_async(cb, 5),
    ).resolves.toEqual([523]);
  });
});
