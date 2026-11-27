// The native bridge's own tests, beside today's fixture tests: many calls at
// once, sleeps and environment reads, errors, refusals, re-entry, leaks, and
// a process that must exit by itself. `scripts/test.sh` runs them again with
// BAML_GC_STRESS=1 (a collection on every allocation).
import "./baml_sdk/index.js";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  BamlError,
  BamlInvalidArgumentError,
  BamlPanic,
  BamlTypeMap,
  getTypeMap,
  setTypeMap,
} from "@boundaryml/baml-bridge";
import { _native } from "@boundaryml/baml-bridge/native";
import {
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
  make_ints,
  make_points,
  make_points_sync,
  nap,
  nap_sync,
  stuck,
  two_lists,
} from "./baml_sdk/bridge_test/index.js";
import { Inner, round_trip_inner } from "./baml_sdk/class_refs/index.js";
import { round_trip_complex_profile } from "./baml_sdk/complex_models/index.js";
import { round_trip_resume } from "./baml_sdk/optional/index.js";

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
    // @ts-expect-error a string where an int is declared
    await expect(add("1", 2)).rejects.toThrow(BamlInvalidArgumentError);
    // @ts-expect-error as above
    await expect(add("1", 2)).rejects.toThrow("argument `a`: expected int, found a string");
    await expect(add(1.5, 2)).rejects.toThrow("argument `a`: expected int, found a number");
    // @ts-expect-error `name` is required
    await expect(round_trip_resume({})).rejects.toThrow(
      "argument `r`: class user.optional.Resume requires field `name`",
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
    expect((caught as { code?: string }).code).toBe("invalid_argument");
    expect((caught as Error).message).toMatch(/refused/);
  });

  it("native_profile_round_trip", async () => {
    const { complexProfile } = await import("./bench_values.js");
    const p = complexProfile();
    expect(await round_trip_complex_profile(p)).toEqual(p);
  });
});

describe("native bridge: the library is not re-entered", () => {
  it("native_a_binding_called_from_a_constructor_throws", async () => {
    const original = getTypeMap();
    let seen: unknown;
    class Hostile {
      constructor(init?: object) {
        try {
          add_sync(1, 2);
        } catch (e) {
          seen = e;
          throw e;
        }
        Object.assign(this, init);
      }
    }
    setTypeMap(
      BamlTypeMap.fromLazyEntries({
        classes: { "user.class_refs.Inner": () => Hostile },
        enums: {},
        typeAliases: {},
      }),
    );
    try {
      await expect(round_trip_inner(new Inner({ value: 1 }))).rejects.toThrow(
        /while the native bridge was copying/,
      );
      expect(String(seen)).toMatch(/while the native bridge was copying/);
    } finally {
      setTypeMap(original);
    }
    // The library is usable again.
    expect(await round_trip_inner(new Inner({ value: 2 }))).toEqual(new Inner({ value: 2 }));
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
