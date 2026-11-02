// Roundtrip coverage for baml_sdk/tuples: a BAML tuple is a TypeScript
// `readonly [A, B]`, represented by an array. Arrays sent to a declared tuple
// slot must have its arity and fit each slot in order; a tuple received with
// no declared tuple type (held as `unknown`, a callback argument, a thrown
// value) keeps its carried type, so sending it back restates the same tuple.
import "./baml_sdk/index.js";
import { describe, it, expect } from "vitest";
import { BamlError, BamlType } from "@boundaryml/baml-bridge";
import {
  TupleHolder,
  TuplePairBox,
  TupleBox,
  round_trip_pair,
  round_trip_pair_async,
  round_trip_single,
  round_trip_repeated,
  round_trip_nested,
  round_trip_optional_slot,
  round_trip_optional_tuple,
  round_trip_tuple_list,
  round_trip_tuple_map,
  round_trip_tuple_or_string,
  round_trip_tuple_arms,
  round_trip_holder,
  round_trip_tuple_box,
  make_pair,
  make_tuple_swapper,
  swap_pair,
  pair_first,
  call_tuple_callback,
  call_tuple_callback_async,
  throw_pair,
  round_trip_wide,
  tuple_as_unknown,
  unknown_has_carried_tuple_type,
  value_has_type,
  call_unknown_tuple_callback_async,
} from "./baml_sdk/tuples/index.js";

describe("roundtrip tuples", () => {
  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_pair", () => {
    expect(round_trip_pair([1, "a"])).toEqual([1, "a"]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_pair_async", async () => {
    expect(await round_trip_pair_async([2, "b"])).toEqual([2, "b"]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_single_and_repeated", () => {
    expect(round_trip_single(["only"])).toEqual(["only"]);
    expect(round_trip_repeated([1, 2, 3])).toEqual([1, 2, 3]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_nested", () => {
    expect(round_trip_nested([[1, "a"], true])).toEqual([[1, "a"], true]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_optional_slot_and_optional_tuple", () => {
    expect(round_trip_optional_slot([1, null])).toEqual([1, null]);
    expect(round_trip_optional_tuple(null)).toBeNull();
    expect(round_trip_optional_tuple([1, "a"])).toEqual([1, "a"]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_containers", () => {
    expect(round_trip_tuple_list([["a", 1], ["b", 2]])).toEqual([["a", 1], ["b", 2]]);
    expect(round_trip_tuple_map({ k: [1, true] })).toEqual({ k: [1, true] });
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_wide", () => {
    const wide = [1, "a", 2, "b", 3, "c", 4, "d", 5, "e", 6, "f", 7] as const;
    expect(round_trip_wide(wide)).toEqual([...wide]);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_union_arms", () => {
    expect(round_trip_tuple_or_string([1, "a"])).toEqual([1, "a"]);
    expect(round_trip_tuple_or_string("s")).toBe("s");
    // Same-arity arms: the slots decide, and a fit in neither order fails.
    expect(round_trip_tuple_arms([1, "a"])).toEqual([1, "a"]);
    expect(round_trip_tuple_arms(["a", 1])).toEqual(["a", 1]);
    expect(() => round_trip_tuple_arms([true, 1] as never)).toThrow();
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_class_fields", () => {
    const holder = new TupleHolder({
      pair: [1, "a"],
      nested: [[2, "b"], false],
      maybe: [3, null],
      pairs: [["x", 4]],
    });
    expect(round_trip_holder(holder)).toEqual(holder);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_round_trip_generic_argument", () => {
    const box = new TupleBox<readonly [number, string]>({ value: [1, "a"] });
    const back = round_trip_tuple_box(box);
    expect(back).toEqual(box);
    // The generic argument crosses back as a tuple type token.
    expect((back as unknown as { $types?: unknown }).$types).toEqual({ T: { tuple: ["int", "string"] } });
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_make_swap_and_index", () => {
    expect(make_pair(1, "a")).toEqual([1, "a"]);
    expect(swap_pair([1, "a"])).toEqual(["a", 1]);
    expect(pair_first([7, "a"])).toBe(7);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_wrong_arity_and_slots_rejected", () => {
    expect(() => round_trip_pair([1] as never)).toThrow();
    expect(() => round_trip_pair([1, "a", 2] as never)).toThrow();
    expect(() => round_trip_pair(["a", 1] as never)).toThrow();
    expect(() => round_trip_optional_slot([1] as never)).toThrow();
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_host_callback_args_and_results", async () => {
    const seen: unknown[] = [];
    const swapped = await call_tuple_callback_async((t) => {
      seen.push(t);
      return [t[1], t[0]];
    }, [1, "a"]);
    expect(seen).toEqual([[1, "a"]]);
    expect(swapped).toEqual(["a", 1]);
    // A callback result of the wrong arity breaks the callback's contract.
    await expect(call_tuple_callback_async(() => ["a"] as never, [1, "a"])).rejects.toThrow();
    // A synchronous call cannot dispatch a host callback.
    expect(() => call_tuple_callback((t) => [t[1], t[0]], [1, "a"])).toThrow();
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_thrown_value", () => {
    try {
      throw_pair([1, "a"]);
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(BamlError);
      expect((error as BamlError).value).toEqual([1, "a"]);
    }
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_dynamic_value_keeps_carried_type", async () => {
    // `(1, true)` is an `(int | string, bool)` by its carried type.
    const held = tuple_as_unknown([1, true]);
    expect(held).toEqual([1, true]);
    expect(unknown_has_carried_tuple_type(held)).toBe(true);
    // A plain array at an open slot is a list, not a tuple.
    expect(unknown_has_carried_tuple_type([1, true])).toBe(false);
    // A callback argument at an open slot, returned at an open slot.
    expect(await call_unknown_tuple_callback_async((v) => v, [1, true])).toBe(true);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_reflected_type_token", () => {
    expect(BamlType.from({ tuple: ["int", "string"] })).toBeInstanceOf(BamlType);
    expect(() => BamlType.from({ tuple: [] })).toThrow();
    // A tuple type token binds a type parameter, in slot order.
    const pair = make_pair(1, "a");
    expect(value_has_type(pair, { $types: { T: { tuple: ["int", "string"] } } })).toBe(true);
    expect(value_has_type(pair, { $types: { T: { tuple: ["string", "int"] } } })).toBe(false);
    expect(value_has_type(pair, { $types: { T: { list: { union: ["int", "string"] } } } })).toBe(false);
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_method_round_trip", async () => {
    const box = new TuplePairBox({ a: 1, b: "a" });
    // Instance bindings are per-instance bound fields, so compare data fields.
    // An instance method returning a tuple, and one taking a tuple.
    expect(box.as_pair()).toEqual([1, "a"]);
    expect(box.with_pair([2, "b"])).toMatchObject({ a: 2, b: "b" });
    // A static method taking a tuple.
    expect(TuplePairBox.from_pair([3, "c"])).toBeInstanceOf(TuplePairBox);
    expect(TuplePairBox.from_pair([3, "c"])).toMatchObject({ a: 3, b: "c" });
    expect(await box.as_pair_async()).toEqual([1, "a"]);
    expect(await box.with_pair_async([4, "d"])).toMatchObject({ a: 4, b: "d" });
    expect(await TuplePairBox.from_pair_async([5, "e"])).toMatchObject({ a: 5, b: "e" });
    expect(() => box.with_pair([1] as never)).toThrow();
  });

  // SDK_PARITY_LINT(skip): C# covers tuples in its own tuples fixture
  it("tuples_function_handle_round_trip", async () => {
    // A BAML closure returned to the host: its handle takes and returns tuples.
    const swap = make_tuple_swapper();
    expect(typeof swap).toBe("function");
    expect(swap([1, "a"])).toEqual(["a", 1]);
    expect(swap([2, "b"])).toEqual(["b", 2]);
    expect(await swap.callAsync([3, "c"])).toEqual(["c", 3]);
    expect(() => swap(["a", 1] as never)).toThrow();
  });
});
