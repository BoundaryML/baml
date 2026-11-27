// The native bridge's own tests, beside today's fixture tests: many calls at
// once, sleeps and environment reads, errors, refusals, re-entry, leaks, and
// a process that must exit by itself. `scripts/test.sh` runs them again with
// BAML_GC_STRESS=1 (a collection on every allocation).
import "./baml_sdk/index.js";
import { spawnSync } from "node:child_process";
import { Worker } from "node:worker_threads";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  BamlClientError,
  BamlError,
  BamlPanic,
  BamlTypeMap,
  getTypeMap,
  setTypeMap,
} from "@boundaryml/baml-bridge";
import { _native } from "@boundaryml/baml-bridge/native";
import {
  Chain,
  LookupFailed,
  Point,
  add,
  add_sync,
  count_points,
  divide,
  env_or,
  explode,
  lookup,
  lookup_sync,
  make_chain,
  make_chain_sync,
  make_ints,
  make_points,
  make_points_sync,
  nap,
  nap_sync,
  round_trip_chain,
  round_trip_point,
  round_trip_point_sync,
  stuck,
  two_lists,
} from "./baml_sdk/bridge_test/index.js";
import { Inner, round_trip_inner, round_trip_inner_sync } from "./baml_sdk/class_refs/index.js";
import { round_trip_complex_profile } from "./baml_sdk/complex_models/index.js";
import { round_trip_resume } from "./baml_sdk/optional/index.js";
import { round_trip_ints } from "./baml_sdk/lists/index.js";
import { round_trip_simple_map } from "./baml_sdk/maps/index.js";

const children = fileURLToPath(new URL("../dist/children/", import.meta.url));

describe("native bridge: calls at once", () => {
  it("native_many_calls_with_promise_all", async () => {
    const started = Date.now();
    const calls: Promise<unknown>[] = [];
    const expected: unknown[] = [];
    for (let i = 0; i < 300; i++) {
      switch (i % 5) {
        case 0:
          calls.push(add(i, 1));
          expected.push(i + 1);
          break;
        case 1:
          calls.push(nap(5 + (i % 40)));
          expected.push(5 + (i % 40));
          break;
        case 2:
          calls.push(env_or("PATH", "unset"));
          expected.push(process.env.PATH);
          break;
        case 3:
          calls.push(env_or("BAML_NATIVE_NO_SUCH_VARIABLE", `fallback ${i}`));
          expected.push(`fallback ${i}`);
          break;
        default:
          calls.push(round_trip_inner(new Inner({ value: i })));
          expected.push(new Inner({ value: i }));
      }
    }
    expect(await Promise.all(calls)).toEqual(expected);
    // The sleeps overlap: one turn's timer serves all of them.
    expect(Date.now() - started).toBeLessThan(2000);
    expect(_native.pending()).toBe(0);
  });

  it("native_sync_call_delivers_the_events_of_other_calls", async () => {
    const sleeping = [nap(30), nap(10), env_or("PATH", "unset")];
    // A sync call drives turns until it ends; the others end during them.
    expect(nap_sync(60)).toBe(60);
    expect(add_sync(2, 3)).toBe(5);
    expect(await Promise.all(sleeping)).toEqual([30, 10, process.env.PATH]);
  });

  it("native_a_timer_is_not_lost_after_an_idle_turn", async () => {
    expect(await add(1, 1)).toBe(2); // a turn with no timer
    expect(await nap(25)).toBe(25); // then one that needs the loop's timer
  });

  it("native_large_results", async () => {
    const points = await make_points(10_000);
    expect(points).toHaveLength(10_000);
    expect(points[9_999]).toEqual(new Point({ x: 9_999, y: 1 }));
    expect(points[0]).toBeInstanceOf(Point);
    expect(await count_points(points)).toBe(10_000);
    const ints = await make_ints(100_000);
    expect(ints[99_999]).toBe(99_999);
    expect(make_points_sync(3)).toEqual([0, 1, 2].map((x) => new Point({ x, y: 3 - x })));
  });

  it("native_one_array_passed_twice_is_copied_twice", async () => {
    const xs = [1, 2, 3];
    expect(await two_lists(xs, xs)).toBe(6);
  });
});

describe("native bridge: how calls end", () => {
  it("native_thrown_declared_error", async () => {
    const error = await lookup("missing").then(
      () => undefined,
      (e: unknown) => e,
    );
    expect(error).toBeInstanceOf(BamlError);
    expect((error as BamlError).className).toBe("user.bridge_test.LookupFailed");
    expect((error as BamlError).value).toEqual(new LookupFailed({ key: "missing", code: 404 }));
    expect((error as BamlError).value).toBeInstanceOf(LookupFailed);
    expect((error as Error).message).toBe("baml error: user.bridge_test.LookupFailed");
    expect(await lookup("there")).toBe("there");
    expect(() => lookup_sync("missing")).toThrow(BamlError);
  });

  it("native_thrown_string", async () => {
    await expect(explode(1)).rejects.toThrow(/^baml error: baml\.error$/);
    await expect(explode(1)).rejects.toMatchObject({ value: "exploded" });
    expect(await explode(0)).toBe(0);
  });

  it("native_panic", async () => {
    const error = await divide(1, 0).then(
      () => undefined,
      (e: unknown) => e,
    );
    expect(error).toBeInstanceOf(BamlPanic);
    expect((error as Error).message).toMatch(/^baml panic: /);
  });

  it("native_stuck_call", async () => {
    await expect(stuck()).rejects.toThrow(BamlPanic);
    await expect(stuck()).rejects.toThrow(/can never finish/);
    // Other calls still run.
    expect(await add(2, 2)).toBe(4);
  });

  it("native_argument_refused_by_the_bindings", async () => {
    // As today's bridge: a plain Error with the code InvalidArg, its message
    // led by the function, the value as today renders it.
    const refused = async (call: Promise<unknown>, message: string): Promise<void> => {
      const error = await call.then(
        () => undefined,
        (e: unknown) => e,
      );
      expect(error).toBeInstanceOf(Error);
      expect((error as { code?: string }).code).toBe("InvalidArg");
      expect((error as Error).message).toBe(message);
    };
    // @ts-expect-error a string where an int is declared
    await refused(add("1", 2), 'user.bridge_test.add: argument `a`: expected int, found string "1"');
    await refused(add(1.5, 2), "user.bridge_test.add: argument `a`: expected int, found number 1.5");
    await refused(
      // @ts-expect-error a string in an int[]
      round_trip_ints([1, "x"]),
      'user.lists.round_trip_ints: argument `xs`[1]: expected int, found string "x"',
    );
    await refused(
      // @ts-expect-error a string where a map value is an int
      round_trip_simple_map({ a: 1, b: "x" }),
      'user.maps.round_trip_simple_map: argument `m`["b"]: expected int, found string "x"',
    );
    await refused(
      // @ts-expect-error `name` is required
      round_trip_resume({}),
      "user.optional.round_trip_resume: argument `r`: class user.optional.Resume requires field `name`",
    );
  });

  it("native_refusal_by_the_library", async () => {
    // The bindings check every argument by its declared type first, so the
    // library's own refusal is reached with an export it does not have.
    let caught: unknown;
    try {
      _native.startRaw(1_000_000, []);
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(Error);
    expect((caught as { code?: string }).code).toBe("InvalidArg");
    expect((caught as Error).message).toMatch(/refused/);
  });

  it("native_profile_round_trip", async () => {
    const { complexProfile } = await import("./bench_values.js");
    const p = complexProfile();
    expect(await round_trip_complex_profile(p)).toEqual(p);
  });
});

/** Run `body` with `user.class_refs.Inner` mapped to `Class`, then restore. */
async function withInnerAs(Class: unknown, body: () => Promise<void>): Promise<void> {
  const original = getTypeMap();
  setTypeMap(
    BamlTypeMap.fromLazyEntries({
      classes: { "user.class_refs.Inner": () => Class },
      enums: {},
      typeAliases: {},
    }),
  );
  try {
    await body();
  } finally {
    setTypeMap(original);
  }
}

const syncError = (body: () => unknown): unknown => {
  try {
    body();
  } catch (e) {
    return e;
  }
  return undefined;
};

describe("native bridge: the library is not re-entered", () => {
  it("native_a_binding_called_from_a_constructor_throws", async () => {
    const seen: unknown[] = [];
    class Hostile {
      constructor(init?: object) {
        add(1, 2).catch((e: unknown) => seen.push(e));
        try {
          add_sync(1, 2);
        } catch (e) {
          seen.push(e);
          throw e;
        }
        Object.assign(this, init);
      }
    }
    await withInnerAs(Hostile, async () => {
      // The value is a plain object: the generated Inner is not mapped here.
      const fromAsync = await round_trip_inner({ value: 1 }).then(
        () => undefined,
        (e: unknown) => e,
      );
      const fromSync = syncError(() => round_trip_inner_sync({ value: 1 }));
      // The same error from both forms, and from the async and sync binding
      // the constructor called.
      for (const error of [fromAsync, fromSync, ...seen]) {
        expect(error).toBeInstanceOf(BamlClientError);
        expect((error as Error).message).toMatch(/while the native bridge held the BAML library/);
        expect((error as { code?: string }).code).toBe("reentrant");
      }
      expect(seen.length).toBeGreaterThanOrEqual(2);
    });
    // The library is usable again.
    expect(await round_trip_inner(new Inner({ value: 2 }))).toEqual(new Inner({ value: 2 }));
  });

  it("native_what_a_constructor_throws_is_thrown_as_it_is_by_both_forms", async () => {
    const thrown: Error[] = [];
    class Throws {
      constructor() {
        const error = new Error("from the constructor");
        thrown.push(error);
        throw error;
      }
    }
    await withInnerAs(Throws, async () => {
      const fromAsync = await round_trip_inner({ value: 1 }).then(
        () => undefined,
        (e: unknown) => e,
      );
      expect(fromAsync).toBe(thrown[thrown.length - 1]);
      const fromSync = syncError(() => round_trip_inner_sync({ value: 1 }));
      expect(fromSync).toBe(thrown[thrown.length - 1]);
    });
  });

  it("native_a_getter_that_throws_while_arguments_are_read", async () => {
    const boom = new Error("boom");
    const point = {
      get x(): number {
        throw boom;
      },
      y: 1,
    };
    await expect(round_trip_point(point)).rejects.toBe(boom);
    expect(syncError(() => round_trip_point_sync(point))).toBe(boom);
    expect(await add(1, 1)).toBe(2);
  });

  it("native_a_getter_that_calls_a_binding_while_arguments_are_read", async () => {
    const point = {
      get x(): number {
        return add_sync(1, 2);
      },
      y: 1,
    };
    const error = await round_trip_point(point).then(
      () => undefined,
      (e: unknown) => e,
    );
    expect(error).toBeInstanceOf(BamlClientError);
    expect((error as Error).message).toMatch(/while the native bridge held the BAML library/);
    expect(await round_trip_point({ x: 1, y: 1 })).toEqual(new Point({ x: 1, y: 1 }));
  });
});

/** A chain of `n` nodes, built in JavaScript. */
function chain(n: number): Chain {
  let c = new Chain({ value: 0, next: null });
  for (let i = 1; i < n; i++) c = new Chain({ value: i, next: c });
  return c;
}

describe("native bridge: depth", () => {
  it("native_a_cyclic_argument_is_refused", async () => {
    const c = new Chain({ value: 1, next: null });
    c.next = c;
    const error = await round_trip_chain(c).then(
      () => undefined,
      (e: unknown) => e,
    );
    expect((error as { code?: string }).code).toBe("InvalidArg");
    expect((error as Error).message).toMatch(
      /^user\.bridge_test\.round_trip_chain: argument `c`(\.next)+: the value is nested too deeply for the boundary, which carries 200 levels$/,
    );
  });

  it("native_an_argument_deeper_than_200_is_refused", async () => {
    await expect(round_trip_chain(chain(250))).rejects.toThrow(/nested too deeply/);
    expect(await round_trip_chain(chain(200))).toEqual(chain(200));
  });

  it("native_a_result_deeper_than_200_is_refused", async () => {
    await expect(make_chain(250)).rejects.toThrow(
      /^the result is nested too deeply for the boundary, which carries 200 levels$/,
    );
    expect(() => make_chain_sync(250)).toThrow(BamlError);
    expect(() => make_chain_sync(250)).toThrow(/nested too deeply/);
    expect(await make_chain(200)).toEqual(chain(200));
    expect(await add(1, 2)).toBe(3);
  });
});

describe("native bridge: nothing leaks", () => {
  it("native_abandoned_and_rejected_calls_free_their_results", async () => {
    await add(0, 0);
    const before = _native.handleCount();
    for (let i = 0; i < 50; i++) {
      void make_points(100); // abandoned: nobody awaits it
      lookup("missing").catch(() => {}); // rejected
      void nap(5); // abandoned, and ends on a later turn
    }
    await nap(40);
    await add(0, 0);
    expect(_native.handleCount()).toBe(before);
  });
});

describe("native bridge: the process", () => {
  it("native_a_finished_script_exits_by_itself", () => {
    // The script's last call has ended; a BAML task still sleeps for ten
    // seconds, and must not keep the process alive.
    const started = Date.now();
    const child = spawnSync(process.execPath, [`${children}exit.js`], {
      encoding: "utf8",
      timeout: 20_000,
    });
    expect(child.stderr).toBe("");
    expect(child.stdout.trim()).toBe("done 3");
    expect(child.status).toBe(0);
    expect(Date.now() - started).toBeLessThan(5_000);
  });

  it("native_a_worker_thread_is_refused_and_the_main_thread_keeps_working", async () => {
    const worker = new Worker(`${children}worker.js`);
    const calls = Promise.all([nap(30), add(1, 2), env_or("PATH", "unset")]);
    const message = await new Promise<string>((resolve) => worker.once("message", resolve));
    expect(message).toMatch(/^BamlClientError: the native BAML library belongs to another JavaScript environment/);
    expect(await calls).toEqual([30, 3, process.env.PATH]);
    await worker.terminate();
    expect(await nap(5)).toBe(5);
    expect(add_sync(2, 2)).toBe(4);
  });

  it("native_a_second_copy_of_the_runtime_is_refused", () => {
    const child = spawnSync(process.execPath, [`${children}dup.js`], {
      encoding: "utf8",
      timeout: 20_000,
    });
    expect(child.stderr).toBe("");
    const lines = child.stdout.trim().split("\n");
    expect(lines[0]).toMatch(/^refused BamlClientError: /);
    expect(lines[1]).toBe("add 3");
    expect(child.status).toBe(0);
  });

  it("native_exit_with_io_and_a_timer_in_flight", () => {
    for (let run = 0; run < 5; run++) {
      const started = Date.now();
      const child = spawnSync(process.execPath, [`${children}io_exit.js`], {
        encoding: "utf8",
        timeout: 20_000,
      });
      expect(child.stderr).toBe("");
      expect(child.stdout.trim()).toBe("done 1 1 3");
      expect(child.status).toBe(0);
      expect(child.signal).toBeNull();
      expect(Date.now() - started).toBeLessThan(5_000);
    }
  });

  it("native_an_unhandled_failure_during_a_sync_call", () => {
    const child = spawnSync(process.execPath, [`${children}unhandled_sync.js`], {
      encoding: "utf8",
      timeout: 20_000,
    });
    expect(child.stdout.trim().split("\n").sort()).toEqual([
      "returned 1",
      "uncaught BamlError baml error: baml.error",
    ]);
    expect(child.status).toBe(0);
  });

  it("native_an_unhandled_failure_is_thrown_on_the_loop", () => {
    const child = spawnSync(process.execPath, [`${children}unhandled.js`], {
      encoding: "utf8",
      timeout: 20_000,
    });
    expect(child.stdout.trim().split("\n").sort()).toEqual([
      "returned 1",
      "uncaught BamlError baml error: baml.error",
    ]);
    expect(child.status).toBe(0);
  });
});
